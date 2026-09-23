fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../icons/icon.ico");
        res.set("ProductName", "Veya");
        res.set("FileDescription", "Veya clipboard flow tracker");
        res.set("CompanyName", "Veya");
        res.compile()
            .expect("failed to embed Windows application icon");
    }
}
