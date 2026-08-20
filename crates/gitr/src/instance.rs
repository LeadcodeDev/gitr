use std::io::{self, Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::{fs, thread};

use ui::persistence;

pub const SOCKET_FILE: &str = "instance.sock";

pub fn socket_path() -> Option<PathBuf> {
    persistence::application_support_dir().map(|dir| dir.join(SOCKET_FILE))
}

pub fn hand_off_to(socket: &Path, repository: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return false;
    };
    let Some(bytes) = repository.to_str().map(str::as_bytes) else {
        return false;
    };
    stream.write_all(bytes).is_ok() && stream.flush().is_ok()
}

pub enum Bound {
    Listening(UnixListener),
    AlreadyRunning,
}

pub fn bind(socket: &Path) -> io::Result<Bound> {
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)?;
    }
    match UnixListener::bind(socket) {
        Ok(listener) => Ok(Bound::Listening(listener)),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).is_ok() {
                return Ok(Bound::AlreadyRunning);
            }
            fs::remove_file(socket)?;
            Ok(Bound::Listening(UnixListener::bind(socket)?))
        }
        Err(error) => Err(error),
    }
}

pub fn serve<F>(listener: UnixListener, on_open: F)
where
    F: Fn(PathBuf) + Send + 'static,
{
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Some(path) = read_request(stream) {
                on_open(path);
            }
        }
    });
}

fn read_request(mut stream: UnixStream) -> Option<PathBuf> {
    let mut request = String::new();
    stream.read_to_string(&mut request).ok()?;
    let trimmed = request.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join(SOCKET_FILE)
    }

    #[test]
    fn handing_off_with_nobody_listening_reports_that_it_did_not() {
        let dir = tempfile::tempdir().expect("fixture must create a temporary directory");
        assert!(!hand_off_to(&socket_in(&dir), Path::new("/tmp/somewhere")));
    }

    #[test]
    fn a_handed_off_path_reaches_the_listener() {
        let dir = tempfile::tempdir().expect("fixture must create a temporary directory");
        let socket = socket_in(&dir);
        let Bound::Listening(listener) = bind(&socket).expect("bind must succeed on a free path")
        else {
            panic!("a free socket path must not report an instance already running");
        };

        let (sender, receiver) = mpsc::channel();
        serve(listener, move |path| {
            let _ = sender.send(path);
        });

        assert!(hand_off_to(&socket, Path::new("/tmp/repository-one")));
        let received = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the listener must receive the handed-off path");
        assert_eq!(received, PathBuf::from("/tmp/repository-one"));
    }

    #[test]
    fn several_paths_each_reach_the_listener() {
        let dir = tempfile::tempdir().expect("fixture must create a temporary directory");
        let socket = socket_in(&dir);
        let Bound::Listening(listener) = bind(&socket).expect("bind must succeed on a free path")
        else {
            panic!("a free socket path must not report an instance already running");
        };

        let (sender, receiver) = mpsc::channel();
        serve(listener, move |path| {
            let _ = sender.send(path);
        });

        for name in ["/tmp/one", "/tmp/two", "/tmp/three"] {
            assert!(hand_off_to(&socket, Path::new(name)));
        }

        let mut received = Vec::new();
        for _ in 0..3 {
            received.push(
                receiver
                    .recv_timeout(Duration::from_secs(5))
                    .expect("every handed-off path must arrive"),
            );
        }
        received.sort();
        assert_eq!(
            received,
            vec![
                PathBuf::from("/tmp/one"),
                PathBuf::from("/tmp/three"),
                PathBuf::from("/tmp/two"),
            ]
        );
    }

    #[test]
    fn a_stale_socket_left_by_a_dead_process_does_not_block_binding() {
        let dir = tempfile::tempdir().expect("fixture must create a temporary directory");
        let socket = socket_in(&dir);
        drop(UnixListener::bind(&socket).expect("fixture must create a socket file"));
        assert!(socket.exists(), "fixture must leave the socket file behind");

        let bound = bind(&socket).expect("a stale socket must not stop a fresh instance");
        assert!(matches!(bound, Bound::Listening(_)));
    }

    #[test]
    fn binding_while_another_instance_listens_reports_it_rather_than_stealing_the_socket() {
        let dir = tempfile::tempdir().expect("fixture must create a temporary directory");
        let socket = socket_in(&dir);
        let Bound::Listening(_held) = bind(&socket).expect("bind must succeed on a free path")
        else {
            panic!("a free socket path must not report an instance already running");
        };

        let second = bind(&socket).expect("a second bind must report rather than fail");
        assert!(matches!(second, Bound::AlreadyRunning));
    }
}
