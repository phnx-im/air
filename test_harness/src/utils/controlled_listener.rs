// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    io::{self, ErrorKind},
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
};

use airserver::{Addressed, IntoStream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_stream::Stream;
use tonic::transport::server::{Connected, TcpConnectInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal = 0,
    DropAll = 1,
}

impl Mode {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Mode::DropAll,
            _ => Mode::Normal,
        }
    }
}

#[derive(Clone)]
pub struct ControlHandle {
    mode: Arc<AtomicU8>,
}

impl ControlHandle {
    fn new(mode: Arc<AtomicU8>) -> Self {
        Self { mode }
    }

    pub fn set_normal(&self) {
        self.mode.store(Mode::Normal as u8, Ordering::Relaxed);
    }

    pub fn set_drop_all(&self) {
        self.mode.store(Mode::DropAll as u8, Ordering::Relaxed);
    }

    pub fn mode(&self) -> Mode {
        Mode::from_u8(self.mode.load(Ordering::Relaxed))
    }
}

/// A TcpStream wrapper that can drop incoming data when in DropAll mode.
///
/// - In Normal mode: behaves like a regular TcpStream (AsyncRead/AsyncWrite).
/// - In DropAll mode:
///     * `poll_read` drains the socket into an internal buffer and discards it
///       (so the kernel buffer doesn't fill),
///       but does NOT deliver any bytes to the caller.
///     * `poll_write` still forwards writes as normal.
pub struct ControlledStream {
    inner: TcpStream,
    mode: Arc<AtomicU8>,
    drop_buf: Box<[u8; 8192]>,
}

impl ControlledStream {
    fn new(inner: TcpStream, mode: Arc<AtomicU8>) -> Self {
        Self {
            inner,
            mode,
            drop_buf: Box::new([0u8; 8192]),
        }
    }

    fn mode(&self) -> Mode {
        Mode::from_u8(self.mode.load(Ordering::Relaxed))
    }

    pub fn inner(&self) -> &TcpStream {
        &self.inner
    }
}

impl AsyncRead for ControlledStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();

        if me.mode() == Mode::DropAll {
            // Fast fail: propagate an error, tonic will close the connection.
            return Poll::Ready(Err(io::Error::new(
                ErrorKind::ConnectionAborted,
                "connection dropped by ControlledStream",
            )));
        }

        Pin::new(&mut me.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ControlledStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        // Writes are always forwarded (we can change this if we want symmetric behaviour).
        Pin::new(&mut me.inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        Pin::new(&mut me.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        Pin::new(&mut me.inner).poll_shutdown(cx)
    }
}

impl Connected for ControlledStream {
    type ConnectInfo = TcpConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.inner.connect_info()
    }
}

pub struct ControlledIncoming {
    listener: TcpListener,
    mode: Arc<AtomicU8>,
}

impl ControlledIncoming {
    pub async fn bind(addr: SocketAddr) -> io::Result<(Self, ControlHandle)> {
        let listener = TcpListener::bind(addr).await?;
        let mode = Arc::new(AtomicU8::new(Mode::Normal as u8));
        let handle = ControlHandle::new(mode.clone());

        Ok((ControlledIncoming { listener, mode }, handle))
    }

    pub fn from_listener(listener: TcpListener) -> (Self, ControlHandle) {
        let mode = Arc::new(AtomicU8::new(Mode::Normal as u8));
        let handle = ControlHandle::new(mode.clone());

        (ControlledIncoming { listener, mode }, handle)
    }

    pub fn inner(&self) -> &TcpListener {
        &self.listener
    }
}

impl Stream for ControlledIncoming {
    type Item = Result<ControlledStream, io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.get_mut();

        match me.listener.poll_accept(cx) {
            // New connection.
            Poll::Ready(Ok((stream, _addr))) => {
                let mode = Mode::from_u8(me.mode.load(Ordering::Relaxed));
                if mode == Mode::DropAll {
                    // Drop the connection and pretend nothing happened.
                    drop(stream);
                    Poll::Pending
                } else {
                    let wrapped = ControlledStream::new(stream, me.mode.clone());
                    Poll::Ready(Some(Ok(wrapped)))
                }
            }
            // Error on accept – surface it.
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            // No connection yet.
            Poll::Pending => Poll::Pending,
        }
    }
}

impl IntoStream for ControlledIncoming {
    type Item = ControlledStream;
    type Error = io::Error;
    type Stream = Self;

    fn into_stream(self) -> Self::Stream {
        self
    }
}

impl Addressed for ControlledIncoming {
    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*; // ControlledIncoming, ControlledStream, ControlHandle, Mode, etc.
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::timeout;
    use tokio_stream::StreamExt; // for .next()

    #[tokio::test(flavor = "current_thread")]
    async fn controlled_stream_and_incoming_drop_connections_in_drop_mode()
    -> Result<(), Box<dyn std::error::Error>> {
        // Bind on an ephemeral port.
        let (mut incoming, ctrl) = ControlledIncoming::bind("127.0.0.1:0".parse().unwrap()).await?;
        let addr = incoming.inner().local_addr()?;

        //
        // 1) Normal mode: connection is yielded and data flows both ways.
        //
        let mut client = TcpStream::connect(addr).await?;

        let item = timeout(Duration::from_secs(1), incoming.next())
            .await
            .expect("timed out waiting for first connection in normal mode");

        let mut server_stream = item
            .expect("incoming ended unexpectedly in normal mode")
            .expect("incoming produced an error in normal mode");

        // Client -> server in normal mode.
        client.write_all(b"hello").await?;

        let mut buf = [0u8; 16];
        let n = timeout(Duration::from_secs(1), server_stream.read(&mut buf))
            .await
            .expect("timed out reading in normal mode")?;

        assert_eq!(&buf[..n], b"hello");

        // Server -> client in normal mode.
        server_stream.write_all(b"pong").await?;

        let mut buf_c = [0u8; 16];
        let n_c = timeout(Duration::from_secs(1), client.read(&mut buf_c))
            .await
            .expect("timed out reading on client in normal mode")?;

        assert_eq!(&buf_c[..n_c], b"pong");

        //
        // 2) DropAll: existing connection should fail fast on read/write.
        //
        ctrl.set_drop_all();

        // Client → server: next read on the server side should immediately error.
        client.write_all(b"world").await?;

        let mut buf2 = [0u8; 16];
        let read_res = server_stream.read(&mut buf2).await;

        assert!(
            read_res.is_err(),
            "server unexpectedly succeeded reading in DropAll fail-fast mode"
        );

        //
        // 3) DropAll: new connections are accepted and dropped, not yielded.
        //
        let _client2 = TcpStream::connect(addr).await?;

        let next_res = timeout(Duration::from_millis(200), incoming.next()).await;

        assert!(
            next_res.is_err(),
            "incoming unexpectedly yielded a new connection in DropAll mode"
        );

        //
        // 4) Back to Normal: new connections behave normally again.
        //
        ctrl.set_normal();

        // Existing server_stream is effectively dead; open a fresh connection.
        let mut client3 = TcpStream::connect(addr).await?;

        let item3 = timeout(Duration::from_secs(1), incoming.next())
            .await
            .expect("timed out waiting for post-DropAll connection")
            .unwrap();

        let mut server_stream2 =
            item3.expect("incoming produced an error after resuming normal mode");

        // Client -> server in normal mode again.
        client3.write_all(b"again").await?;

        let mut buf3 = [0u8; 16];
        let n3 = timeout(Duration::from_secs(1), server_stream2.read(&mut buf3))
            .await
            .expect("timed out reading after resuming normal mode")?;

        assert_eq!(&buf3[..n3], b"again");

        // Server -> client in normal mode again.
        server_stream2.write_all(b"back").await?;

        let mut buf_c3 = [0u8; 16];
        let n_c3 = timeout(Duration::from_secs(1), client3.read(&mut buf_c3))
            .await
            .expect("timed out reading on client after resuming normal mode")?;

        assert_eq!(&buf_c3[..n_c3], b"back");

        Ok(())
    }
}
