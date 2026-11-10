fn main() {
    // Embed the app icon in the Windows .exe (Explorer, taskbar pins, Alt+Tab).
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("embed Windows icon");
    }
}
