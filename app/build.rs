fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).unwrap();

    // Icon and version details shown by Explorer, the taskbar and the installer.
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../assets/altr.ico");
    res.set("ProductName", "ALTR");
    res.set("FileDescription", "ALTR");
    res.compile().unwrap();
}
