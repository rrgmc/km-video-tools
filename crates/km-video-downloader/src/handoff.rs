//! Handing a list to the copy of this program that is already running.
//!
//! # The failure this exists to remove
//!
//! A double-click starts a *new* process. If one is already running, the new one finds the port
//! taken and cannot serve — and on Windows the executable that gets double-clicked is the
//! GUI-subsystem one, whose standard error goes nowhere at all. So without this module the second
//! double-click of somebody's afternoon does nothing, says nothing, and leaves no trace anywhere
//! they would think to look.
//!
//! **A list is not what makes that happen, which is why one is optional here.** The first version of
//! this module handed over only when a file association had passed a path, and so answered the
//! second double-click of a *list* while leaving the second double-click of the *program* exactly
//! where it had been — the commoner of the two, and silent on both platforms. macOS made the gap
//! plain rather than causing it: it delivers a document as an Apple Event, so the positional
//! argument is empty there even on the launch that opened a file, and the whole branch was
//! unreachable on that platform.
//!
//! So the rule is now the plain one. **A copy that cannot have the port hands over whatever it was
//! opened with, including nothing at all**, and the copy with the window comes forward.
//!
//! # The port is the handoff
//!
//! There is no second mechanism here — no named pipe, no lock file, no single-instance mutex. The
//! program that is already running is already an HTTP server on a known port, so the copy that
//! cannot start posts its path to it and stops. The instance with the window is the instance that
//! answers, which is the right way round.
//!
//! # Written by hand, over a `TcpStream`
//!
//! One POST to loopback, four headers, a body of one field. There is no HTTP client in this
//! dependency tree and this is not the reason to add one: `reqwest` would bring a TLS stack and a
//! second async runtime into a program whose whole build story is a handful of crates and no C
//! compiler, in order to talk to a socket on this same machine.
//!
//! `std::net` is synchronous, which is what is wanted — this runs before the runtime is doing
//! anything and the process exits on the answer.

use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::server::OPENED_MARK;

/// How long to wait on a program on this same machine.
///
/// Generous for loopback and short enough that a socket held open by something that will never
/// answer does not leave a double-clicked file apparently hanging. Set on the connect *and* on the
/// read: a port opened by something that never replies is exactly the shape a stuck handoff has.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Enough of an answer to recognise, and a cap on anything else.
///
/// Whatever has the port may be a program that answers a POST with a megabyte of HTML. The reply
/// this looks for is one short line, so nothing beyond this is worth reading.
const REPLY_LIMIT: usize = 8 * 1024;

/// Tells the copy already listening that this one was opened, and says whether it answered.
///
/// **The list is optional and its absence is not a lesser case.** `None` is the plain second launch
/// — somebody opening the program while it is already open — and it means *come forward*. `Some` is
/// that same message carrying a document to show once it does.
///
/// **Loopback rather than whatever was asked for.** The bind that failed may have been `0.0.0.0`,
/// which is a way of listening and not an address to connect to — and a handoff is by definition to
/// a program on this machine.
pub fn hand_over(port: u16, list: Option<&Path>) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let body = match list {
        Some(list) => format!("path={}", encode(&list.display().to_string())),
        None => String::new(),
    };

    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)
        .with_context(|| format!("connecting to {addr}"))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    // `Connection: close` so the far side ends the body by ending the stream, and this needs to
    // understand neither `Content-Length` nor chunked framing to read the answer.
    let request = format!(
        "POST /opened HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .context("sending it over")?;
    stream.flush().context("sending it over")?;

    let mut answer = String::new();
    stream
        .take(REPLY_LIMIT as u64)
        .read_to_string(&mut answer)
        .context("reading the answer")?;

    // **Identity first, outcome second, and in that order.** The marker says the answer came from
    // this program — any other server could have this port, and handing a path to one would be a
    // silent success that fetched nothing. Only once that is settled does the status mean anything,
    // and a refusal from a copy of this program is worth repeating word for word: it was written
    // for a person and it names what was actually wrong.
    if !answer.contains(OPENED_MARK) {
        bail!("something else is listening on {addr}");
    }
    if !answer.starts_with("HTTP/1.1 200") {
        bail!("{}", said(&answer));
    }
    Ok(())
}

/// What the far end said, out of the response it said it in.
///
/// The body is what follows the blank line; anything unrecognisable falls back to the whole answer,
/// because a mangled reply is better read in full than summarised into nothing.
fn said(answer: &str) -> &str {
    const BLANK_LINE: &str = "\r\n\r\n";
    answer
        .split_once(BLANK_LINE)
        .map_or(answer, |(_, body)| body)
        .trim()
}

/// Percent-encoding, for the one field this sends.
///
/// **Everything but the unreserved set**, which is deliberately blunt: a Windows path is full of
/// backslashes, colons and spaces, and a list of exceptions is a list of things to get wrong.
/// `handlers::decode` is the other half and takes any of it.
pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Windows path survives the encoder, which is the only body this ever sends.
    ///
    /// The round trip is asserted against `handlers`' own decoder rather than against a literal,
    /// because what matters is that the two agree — a matching pair of wrong implementations would
    /// still be wrong, but a mismatched pair is the failure that would actually happen.
    #[test]
    fn a_windows_path_survives_the_encoding_this_sends_it_with() {
        for path in [
            r"C:\Users\Someone\My Songs\anime.kmvf",
            r"\\server\share\a+b\list.kmvf",
            "/home/someone/songs/リスト.kmvf",
            "C:/songs/100% done.kmvf",
        ] {
            let encoded = encode(path);
            assert!(
                !encoded.contains(['&', '=', '+', ' ']),
                "{encoded} would be read back as two fields"
            );
            assert_eq!(crate::handlers::decode(&encoded), path);
        }
    }
}
