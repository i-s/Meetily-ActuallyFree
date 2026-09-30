//! PCM input and diagnostic output must make progress concurrently.
use std::io::Write;
use std::process::{Child, Output};

pub(super) fn write_audio_and_wait(mut child: Child, data: &[u8]) -> anyhow::Result<Output> {
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("Missing encoder stdin");
    };
    // Writing PCM first can deadlock: FFmpeg fills its diagnostic pipe and
    // stops reading stdin while the parent is still blocked writing audio.
    // A scoped writer borrows the PCM without another recording-sized copy;
    // wait_with_output drains stdout/stderr concurrently and reaps the child.
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(data));
        let output = child.wait_with_output();
        let written = writer.join().map_err(|_| anyhow::anyhow!("Encoder input writer panicked"))?;
        let output = output?;
        if output.status.success() {
            written?;
        }
        // On an encoder failure preserve stderr and exit status for the caller,
        // rather than hiding its real error behind the resulting broken pipe.
        Ok(output)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::{Command, Stdio};

    #[test]
    fn encoder_child() {
        if std::env::var_os("MEETILY_ENCODER_TEST_REJECT").is_some() {
            eprintln!("synthetic encoder rejection");
            std::process::exit(7);
        }
        let Some(destination) = std::env::var_os("MEETILY_ENCODER_TEST_OUTPUT") else { return; };
        // Break a regressed parent/child pipe deadlock without hanging CI.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(10));
            std::process::exit(99);
        });
        std::io::stderr().write_all(&vec![b'x'; 1024 * 1024]).unwrap();
        let mut received = Vec::new();
        std::io::stdin().read_to_end(&mut received).unwrap();
        std::fs::write(destination, received).unwrap();
    }

    #[test]
    fn encoder_failure_retains_diagnostics_and_exit_status() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{}::encoder_child", module_path!().split_once("::").unwrap().1), "--nocapture"])
            .env("MEETILY_ENCODER_TEST_REJECT", "1")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().unwrap();
        let output = write_audio_and_wait(child, &vec![0; 2 * 1024 * 1024]).unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stderr).contains("synthetic encoder rejection"));
    }

    #[test]
    fn drains_diagnostics_while_writing_all_pcm_and_reaps_child() {
        let destination = std::env::temp_dir().join(format!("meetily-encoder-test-{}-{}.pcm", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{}::encoder_child", module_path!().split_once("::").unwrap().1), "--nocapture"])
            .env("MEETILY_ENCODER_TEST_OUTPUT", &destination)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().unwrap();
        let data = vec![0x45; 2 * 1024 * 1024];
        let result = write_audio_and_wait(child, &data);
        let saved = std::fs::read(&destination);
        let _ = std::fs::remove_file(&destination);
        let output = result.expect("Encoder pipes must make progress concurrently");
        assert!(output.status.success(), "encoder exited with {}", output.status);
        assert!(output.stderr.len() >= 1024 * 1024);
        assert_eq!(saved.unwrap(), data, "all PCM, including the tail, must reach encoder");
    }
}
