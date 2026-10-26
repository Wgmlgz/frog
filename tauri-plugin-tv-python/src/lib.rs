use serde::{Deserialize, Serialize};
use tauri::{plugin::{Builder, PluginHandle, TauriPlugin}, Manager, Runtime};

#[derive(Serialize)]
struct Request<'a> {
    json: &'a str,
}

#[derive(Deserialize)]
struct Response {
    json: String,
}

#[derive(Deserialize)]
struct ClipboardText {
    text: String,
}

pub struct TvPython<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> TvPython<R> {
    pub fn request(&self, json: &str) -> Result<String, tauri::plugin::mobile::PluginInvokeError> {
        let response: Response = self.0.run_mobile_plugin("request", Request { json })?;
        Ok(response.json)
    }

    pub fn refresh_widgets(&self) -> Result<(), tauri::plugin::mobile::PluginInvokeError> {
        self.0.run_mobile_plugin::<()>("refreshWidgets", ())
    }


    pub fn read_clipboard_text(&self) -> Result<String, tauri::plugin::mobile::PluginInvokeError> {
        let response: ClipboardText = self.0.run_mobile_plugin("readClipboardText", ())?;
        Ok(response.text)
    }
}

pub trait TvPythonExt<R: Runtime> {
    fn tv_python(&self) -> &TvPython<R>;
}

impl<R: Runtime, T: Manager<R>> TvPythonExt<R> for T {
    fn tv_python(&self) -> &TvPython<R> {
        self.state::<TvPython<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("tv-python")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.wgmlgz.frog.tvpython", "TvPythonPlugin")?;
            app.manage(TvPython(handle));
            Ok(())
        })
        .build()
}
