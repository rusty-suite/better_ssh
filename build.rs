fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "BetterSSH");
        res.set("FileDescription", "Modern SSH Client");
        res.set("LegalCopyright", "Copyright 2024 BetterSSH Contributors");
        if let Err(e) = res.compile() {
            eprintln!("winres error: {e}");
        }
    }
}
