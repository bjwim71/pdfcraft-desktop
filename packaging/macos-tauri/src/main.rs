//! The outer app receives Launch Services events through Tauri. The bundled editor owns the
//! actual window, native menus and document state. Its winit event loop lives in its own process.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::Duration,
};

use tauri::{AppHandle, RunEvent, Url};

#[derive(Debug)]
enum Request {
    Start,
    Activate,
    Open(Vec<PathBuf>),
}

fn main() {
    if let Err(error) = run() {
        report_error(&format!("PdfCraft couldn't start. {error}"));
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let contents = executable.parent().and_then(Path::parent).ok_or("missing app bundle")?;
    let editor = contents.join("Helpers/PdfCraft.app");
    if !editor.join("Contents/MacOS/PdfCraft").is_file() {
        return Err("The bundled editor is missing. Reinstall PdfCraft from the disk image.".into());
    }

    let (requests, inbox) = mpsc::channel();
    let mut app = tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::Builder::new().name("pdfcraft-launch".into()).spawn(move || supervise(&editor, &inbox, &handle))?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    app.run(move |_, event| {
        let request = match event {
            RunEvent::Ready => Request::Start,
            RunEvent::Reopen { .. } => Request::Activate,
            RunEvent::Opened { urls } => {
                let paths = local_paths(urls);
                if paths.is_empty() {
                    return;
                }
                Request::Open(paths)
            }
            _ => return,
        };
        let _ = requests.send(request);
    });
    Ok(())
}

fn local_paths(urls: Vec<Url>) -> Vec<PathBuf> {
    urls.into_iter().filter_map(|url| url.to_file_path().ok()).collect()
}

fn launch(editor: &Path, paths: &[PathBuf], wait: bool) -> Command {
    let mut command = Command::new("/usr/bin/open");
    if wait {
        command.arg("-W");
    }
    command.arg("-a").arg(editor).args(paths);
    command
}

fn supervise(editor: &Path, inbox: &Receiver<Request>, app: &AppHandle) {
    let mut waiter: Option<Child> = None;
    loop {
        match inbox.recv_timeout(Duration::from_millis(250)) {
            Ok(Request::Start) if waiter.is_some() => {}
            Ok(request) => {
                let paths = match &request {
                    Request::Open(paths) => paths.as_slice(),
                    _ => &[],
                };
                if let Err(error) = dispatch(editor, paths, &mut waiter) {
                    report_error(&format!("PdfCraft couldn't open the editor. {error}"));
                    if waiter.is_none() {
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if let Some(child) = &mut waiter {
                    match child.try_wait() {
                        Ok(None) => {}
                        Ok(Some(status)) if status.success() => break,
                        Ok(Some(status)) => {
                            report_error(&format!("macOS couldn't launch the PdfCraft editor ({status})."));
                            break;
                        }
                        Err(error) => {
                            report_error(&format!("PdfCraft couldn't track the editor. {error}"));
                            break;
                        }
                    }
                }
            }
        }
    }
    app.exit(0);
}

fn dispatch(editor: &Path, paths: &[PathBuf], waiter: &mut Option<Child>) -> io::Result<()> {
    let mut command = launch(editor, paths, waiter.is_none());
    if waiter.is_none() {
        // Keep the outer app alive to receive further Finder opens until the editor quits.
        *waiter = Some(command.spawn()?);
    } else {
        let output = command.output()?;
        if !output.status.success() {
            return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).trim()));
        }
    }
    Ok(())
}

fn report_error(message: &str) {
    let _ = writeln!(io::stderr().lock(), "{message}");
    rfd::MessageDialog::new().set_title("PdfCraft").set_level(rfd::MessageLevel::Error).set_description(message).show();
}

#[cfg(test)]
mod tests {
    use super::{PathBuf, Url, launch, local_paths};

    #[test]
    fn accepts_only_local_file_urls_and_preserves_finder_order() -> Result<(), Box<dyn std::error::Error>> {
        let urls = [
            "file:///tmp/first%20file.pdf",
            "https://example.com/remote.pdf",
            "file://server/tmp/remote.pdf",
            "file:///tmp/%E6%97%A5%E6%9C%AC%23.pdf",
        ]
        .into_iter()
        .map(Url::parse)
        .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(local_paths(urls), [PathBuf::from("/tmp/first file.pdf"), PathBuf::from("/tmp/日本#.pdf")]);
        Ok(())
    }

    #[test]
    fn passes_each_path_as_its_own_argument_to_launch_services() {
        let editor = PathBuf::from("/Applications/PdfCraft.app/Contents/Helpers/PdfCraft.app");
        let paths = [PathBuf::from("/tmp/first file.pdf"), PathBuf::from("/tmp/-n.pdf")];
        let command = launch(&editor, &paths, true);
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
            ["-W", "-a", "/Applications/PdfCraft.app/Contents/Helpers/PdfCraft.app", "/tmp/first file.pdf", "/tmp/-n.pdf"]
        );
    }
}
