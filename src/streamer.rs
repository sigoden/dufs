use async_stream::stream;
use futures::{Stream, StreamExt};
use std::io::Error;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct Streamer<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader: R,
    buf_size: usize,
}

impl<R> Streamer<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    #[inline]
    pub fn new(reader: R, buf_size: usize) -> Self {
        Self { reader, buf_size }
    }
    pub fn into_stream(
        mut self,
    ) -> Pin<Box<impl ?Sized + Stream<Item = Result<Vec<u8>, Error>> + 'static>> {
        let stream = stream! {
            loop {
                let mut buf = vec![0; self.buf_size];
                let r = self.reader.read(&mut buf).await?;
                if r == 0 {
                    break
                }
                buf.truncate(r);
                yield Ok(buf);
            }
        };
        stream.boxed()
    }
    // allow truncation as truncated remaining is always less than buf_size: usize
    pub fn into_stream_sized(
        mut self,
        max_length: u64,
    ) -> Pin<Box<impl ?Sized + Stream<Item = Result<Vec<u8>, Error>> + 'static>> {
        let stream = stream! {
        let mut remaining = max_length;
            loop {
                if remaining == 0 {
                    break;
                }
                let bs = if remaining >= self.buf_size as u64 {
                    self.buf_size
                } else {
                    remaining as usize
                };
                let mut buf = vec![0; bs];
                let r = self.reader.read(&mut buf).await?;
                if r == 0 {
                    break;
                } else {
                    buf.truncate(r);
                    yield Ok(buf);
                }
                remaining -= r as u64;
            }
        };
        stream.boxed()
    }
}
