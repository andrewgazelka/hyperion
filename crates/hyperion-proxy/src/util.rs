use std::io::IoSlice;

use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Extension trait for [`AsyncWrite`] to write all data from given IO vectors.
pub trait AsyncWriteVectoredExt: AsyncWrite + Unpin {
    /// Writes all data from the given IO vectors to the writer.
    fn write_vectored_all(
        &mut self,
        mut io_vectors: &mut [IoSlice<'_>],
    ) -> impl std::future::Future<Output = std::io::Result<()>> {
        async move {
            while !io_vectors.is_empty() {
                let bytes_written = self.write_vectored(io_vectors).await?;
                if bytes_written == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "failed to write the entire buffer to the writer",
                    ));
                }
                IoSlice::advance_slices(&mut io_vectors, bytes_written);
            }
            Ok(())
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWriteVectoredExt for T {}
