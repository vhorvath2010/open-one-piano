//! Bluetooth worker: owns the BLE connection on a background thread so the UI never blocks.

use crate::protocol::{self, Layer, Reply, Tone};
use btleplug::api::{
    Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone, Debug)]
pub struct Device {
    pub id: PeripheralId,
    pub name: String,
}

pub enum Command {
    Scan,
    Connect(Device),
    SetTone(Tone),
    Disconnect,
}

pub enum Event {
    Status(String),
    Found(Device),
    ScanDone,
    Connected(String),
    Settings(protocol::Settings),
    Disconnected,
    Error(String),
}

type Emit = Arc<dyn Fn(Event) + Send + Sync>;

struct Link {
    peripheral: Peripheral,
    midi: Characteristic,
    task: tokio::task::JoinHandle<()>,
}

/// Start the worker. `notify` is called after every event so the UI can repaint.
pub fn spawn(
    events: Sender<Event>,
    notify: impl Fn() + Send + Sync + 'static,
) -> UnboundedSender<Command> {
    let (tx, rx) = unbounded_channel();
    std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            let emit: Emit = Arc::new(move |e: Event| {
                let _ = events.send(e);
                notify();
            });
            rt.block_on(run(rx, emit));
        })
        .expect("spawn bluetooth thread");
    tx
}

async fn run(mut rx: UnboundedReceiver<Command>, emit: Emit) {
    let adapter = match first_adapter().await {
        Ok(a) => a,
        Err(e) => {
            emit(Event::Error(e));
            return;
        }
    };
    watch_disconnects(&adapter, emit.clone()).await;

    let mut link: Option<Link> = None;
    while let Some(cmd) = rx.recv().await {
        let result = match cmd {
            Command::Scan => scan(&adapter, &emit).await,
            Command::Connect(dev) => {
                drop_link(&mut link).await;
                emit(Event::Status(format!("Connecting to {}…", dev.name)));
                match connect(&adapter, &dev, emit.clone()).await {
                    Ok(l) => {
                        link = Some(l);
                        emit(Event::Connected(dev.name));
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            Command::SetTone(tone) => match &link {
                Some(l) => set_tone(l, tone).await,
                None => Err("Not connected".into()),
            },
            Command::Disconnect => {
                drop_link(&mut link).await;
                emit(Event::Disconnected);
                Ok(())
            }
        };
        if let Err(e) = result {
            emit(Event::Error(e));
        }
    }
}

async fn first_adapter() -> Result<Adapter, String> {
    let manager = Manager::new()
        .await
        .map_err(|e| format!("Bluetooth unavailable: {e}"))?;
    manager
        .adapters()
        .await
        .map_err(|e| format!("Bluetooth unavailable: {e}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "No Bluetooth adapter found. Is Bluetooth turned on?".into())
}

async fn watch_disconnects(adapter: &Adapter, emit: Emit) {
    if let Ok(mut events) = adapter.events().await {
        tokio::spawn(async move {
            while let Some(ev) = events.next().await {
                if let CentralEvent::DeviceDisconnected(_) = ev {
                    emit(Event::Disconnected);
                }
            }
        });
    }
}

async fn scan(adapter: &Adapter, emit: &Emit) -> Result<(), String> {
    emit(Event::Status("Looking for keyboards…".into()));
    adapter
        .start_scan(ScanFilter {
            services: vec![protocol::MIDI_SERVICE],
        })
        .await
        .map_err(|e| format!("Couldn't scan: {e}. Is Bluetooth turned on?"))?;
    let mut seen = HashSet::new();
    for _ in 0..12 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        for p in adapter.peripherals().await.unwrap_or_default() {
            let Ok(Some(props)) = p.properties().await else {
                continue;
            };
            if !props.services.contains(&protocol::MIDI_SERVICE) {
                continue;
            }
            let Some(name) = props.local_name.or(props.advertisement_name) else {
                continue;
            };
            if seen.insert(p.id()) {
                emit(Event::Found(Device { id: p.id(), name }));
            }
        }
    }
    let _ = adapter.stop_scan().await;
    emit(Event::ScanDone);
    Ok(())
}

async fn connect(adapter: &Adapter, dev: &Device, emit: Emit) -> Result<Link, String> {
    let peripheral = adapter
        .peripheral(&dev.id)
        .await
        .map_err(|e| format!("Keyboard not found: {e}"))?;
    if !peripheral.is_connected().await.unwrap_or(false) {
        tokio::time::timeout(Duration::from_secs(15), peripheral.connect())
            .await
            .map_err(|_| {
                "Timed out connecting. Close the ONE app on your phone and try again.".to_string()
            })?
            .map_err(|e| {
                format!("Couldn't connect: {e}. Close the ONE app on your phone and try again.")
            })?;
    }
    peripheral
        .discover_services()
        .await
        .map_err(|e| format!("Couldn't read the keyboard's services: {e}"))?;
    let midi = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == protocol::MIDI_CHAR)
        .ok_or("This device doesn't have Bluetooth MIDI.")?;
    peripheral
        .subscribe(&midi)
        .await
        .map_err(|e| format!("Couldn't listen to the keyboard: {e}"))?;

    let mut stream = peripheral
        .notifications()
        .await
        .map_err(|e| e.to_string())?;
    let task = tokio::spawn(async move {
        while let Some(n) = stream.next().await {
            debug(format!("IN  {} {}", n.uuid, hex(&n.value)));
            for reply in protocol::parse_notification(&n.value) {
                match reply {
                    Reply::Hello => {}
                    Reply::Settings(s) => emit(Event::Settings(s)),
                }
            }
        }
    });

    let link = Link {
        peripheral,
        midi,
        task,
    };
    write(&link, &protocol::hello()).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    write(&link, &protocol::query_settings()).await?;
    Ok(link)
}

async fn set_tone(link: &Link, tone: Tone) -> Result<(), String> {
    // Make sure the main layer is on (otherwise the keys stay silent), then pick the tone.
    write(link, &protocol::layer_switch(Layer::Main, true)).await?;
    tokio::time::sleep(Duration::from_millis(60)).await;
    write(link, &protocol::select_tone(Layer::Main, tone)).await?;
    tokio::time::sleep(Duration::from_millis(80)).await;
    write(link, &protocol::query_settings()).await
}

fn debug(msg: impl AsRef<str>) {
    if std::env::var_os("OPEN_ONE_PIANO_DEBUG").is_some() {
        eprintln!("[ble] {}", msg.as_ref());
    }
}

fn hex(d: &[u8]) -> String {
    d.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn write(link: &Link, data: &[u8]) -> Result<(), String> {
    debug(format!("OUT {}", hex(data)));
    link.peripheral
        .write(&link.midi, data, WriteType::WithoutResponse)
        .await
        .map_err(|e| format!("Couldn't send to the keyboard: {e}"))
}

async fn drop_link(link: &mut Option<Link>) {
    if let Some(l) = link.take() {
        l.task.abort();
        let _ = l.peripheral.disconnect().await;
    }
}
