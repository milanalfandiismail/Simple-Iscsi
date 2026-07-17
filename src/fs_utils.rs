use std::fs::File;
use std::io;

#[cfg(windows)]
use std::os::windows::fs::FileExt;

pub fn file_read_exact_at(file: &File, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
    #[cfg(windows)]
    {
        while !buf.is_empty() {
            match file.seek_read(buf, offset) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "failed to fill whole buffer"))
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        unimplemented!("Only windows is supported for concurrent I/O")
    }
}

pub fn file_write_all_at(file: &File, mut offset: u64, mut buf: &[u8]) -> io::Result<()> {
    #[cfg(windows)]
    {
        while !buf.is_empty() {
            match file.seek_write(buf, offset) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "failed to write whole buffer")),
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        unimplemented!("Only windows is supported for concurrent I/O")
    }
}
