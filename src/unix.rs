use hyper::server::accept::Accept;
use tokio::net::UnixListener;

use std::pin::Pin;
use std::task::{Context, Poll};

pub struct UnixAcceptor {
    inner: UnixListener,
}

impl UnixAcceptor {
    pub fn from_listener(listener: UnixListener) -> Self {
        Self { inner: listener }
    }
}

impl Accept for UnixAcceptor {
    type Conn = tokio::net::UnixStream;
    type Error = std::io::Error;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        match self.inner.poll_accept(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok((socket, _addr))) => Poll::Ready(Some(Ok(socket))),
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
        }
    }
}
