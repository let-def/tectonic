//! TODO

// use super::{InputHandle, IoProvider, OpenResult, OutputHandle};
use std::{rc::Rc, cell::RefCell, io, env};
use libc::wait;
use texpresso_protocol as txp;
use tectonic_status_base::StatusBackend;
use crate::{IoProvider, OpenResult, InputHandle, OutputHandle,
            InputFeatures, SeekFrom, InputOrigin};

/// TODO
pub struct TexpressoIOState {
    client : txp::Client,
    released: Vec<txp::FileId>,
    next_id: txp::FileId,
    gen: usize,
    last_passed_open: String,
}

/// TODO
pub type TexpressoIO = Rc<RefCell<TexpressoIOState>>;

/// TODO
pub struct TexpressoReader {
    io: TexpressoIO,
    id: txp::FileId,
    abs_pos: usize,
    buf: [u8; 1024],
    buf_pos: u32,
    buf_len: u32,
    size: Option<usize>,
    gen: usize,
}

impl TexpressoReader {
    fn get_file_size(&mut self) -> usize {
        match self.size {
            Some(size) => size,
            None => {
                let mut io = self.io.borrow_mut();
                let size = io.client.size(self.id) as usize;
                self.size = Some(size);
                size
            }
        }
    }
}

impl io::Read for TexpressoReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut io = self.io.borrow_mut();
        if io.gen != self.gen {
            self.abs_pos += self.buf_pos as usize;
            self.buf_pos = 0;
            self.buf_len = 0;
            self.gen = io.gen;
        }
        if self.buf_pos == self.buf_len {
            let abs_pos = self.abs_pos + self.buf_pos as usize;
            loop {
                match io.client.read(self.id, abs_pos as u32, &mut self.buf) {
                    Some(size) => {
                        self.abs_pos = abs_pos;
                        self.buf_pos = 0;
                        self.buf_len = size as u32;
                        break;
                    }
                    None => {
                        io.client.flush();
                        io.gen += 1;
                        let child = unsafe { io.client.fork() };
                        if child == 0 {
                            io.client.child(unsafe{libc::getpid()})
                        } else {
                            let mut status : i32 = 1;
                            let result =
                                unsafe { wait(std::ptr::addr_of_mut!(status)) };
                            if result == -1 {
                                panic!("TeXpresso: fork: error while waiting for child");
                            };
                            if result != child {
                                panic!("TeXpresso: fork: unexpected pid");
                            };
                            let resume = io.client.back(unsafe{libc::getpid()}, child, status as u32);
                            if !resume {
                                std::process::exit(1)
                            }
                        }
                    }
                }
            }
        };
        let len = buf.len();
        let pos = self.buf_pos as usize;
        let rem = self.buf_len as usize - pos;
        let n = if rem >= len {
            buf.copy_from_slice(&self.buf[pos..pos + len]);
            self.buf_pos += len as u32;
            len
        } else {
            buf[0..rem].copy_from_slice(&self.buf[pos..pos + rem]);
            self.buf_pos = self.buf_len;
            rem
        };
        io.client.seen(self.id, self.abs_pos as u32 + self.buf_pos);
        Ok(n)
    }

}

impl InputFeatures for TexpressoReader {

    fn get_size(&mut self) -> tectonic_errors::Result<usize> {
        Ok(self.get_file_size())
    }

    fn try_seek(&mut self, pos: SeekFrom) -> tectonic_errors::Result<u64> {
        let size = self.get_file_size();
        let pos = match pos {
            SeekFrom::Start(ofs) =>
                ofs as usize,
            SeekFrom::Current(ofs) =>
                (self.abs_pos as i64 + self.buf_pos as i64+ ofs) as usize,
            SeekFrom::End(ofs) => {
                if ofs as usize > size {
                    panic!("TODO Find a way to return an error :D");
                };
                (size - ofs as usize) as usize
            }
        };
        if pos > size {
            panic!("TODO Find a way to return an error :D");
        }
        self.abs_pos = pos;
        self.buf_pos = 0;
        self.buf_len = 0;
        Ok(pos as u64)
    }

}

/// TODO
pub struct TexpressoWriter {
    io: TexpressoIO,
    id: txp::FileId,
    pos: usize,
}

impl TexpressoIOState {
    /// TODO
    pub fn new(client: txp::Client) -> TexpressoIOState {
        TexpressoIOState {
            client,
            released: Vec::new(),
            next_id: 0,
            gen: 0,
            last_passed_open: "".to_string()
        }
    }

    /// TODO
    pub fn new_texpresso_io(client: txp::Client) -> TexpressoIO {
        TexpressoIO::new(RefCell::new(Self::new(client)))
    }

    /// TODO
    pub unsafe fn client_from_env() -> Option<txp::Client> {
        match env::var("TEXPRESSO_FD") {
            Ok(val) => Some(txp::Client::connect_raw_fd(val.parse::<i32>().unwrap())),
            Err(_) => None
        }
    }

    /// TODO
    pub unsafe fn new_from_raw_fd(fd: std::os::unix::io::RawFd) -> TexpressoIOState {
        Self::new(txp::Client::connect_raw_fd(fd))
    }

    fn alloc_id(&mut self) -> txp::FileId {
        let id = match self.released.pop() {
            Some(id) => id,
            None => {
                let result = self.next_id;
                if result >= 1024 {
                    panic!("texpresso: Out of file ids");
                };
                self.next_id = result + 1;
                result
            }
        };
        // eprintln!("alloc_id {id}");
        id
    }

    fn release_id(&mut self, id: txp::FileId) {
        // eprintln!("release_id {id}");
        self.released.push(id);
    }
}

impl Drop for TexpressoReader {
    fn drop(&mut self) {
        let mut io = self.io.borrow_mut();
        io.client.close(self.id);
        io.release_id(self.id)
    }
}

impl Drop for TexpressoWriter {
    fn drop(&mut self) {
        let mut io = self.io.borrow_mut();
        io.client.close(self.id);
        io.release_id(self.id)
    }
}

impl io::Write for TexpressoWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut io = self.io.borrow_mut();
        io.client.write(self.id, self.pos as u32, buf);
        let len = buf.len();
        self.pos += len;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut io = self.io.borrow_mut();
        io.client.flush();
        Ok(())
    }
}

impl IoProvider for TexpressoIO {
    /// Open the named file for output.
    fn output_open_name(&mut self, name: &str) -> OpenResult<OutputHandle> {
        let (id, open) = {
            let mut io = self.borrow_mut();
            let id = io.alloc_id();
            let open = io.client.open(id, name, "w");
            if !open { io.release_id(id); };
            (id, open)
        };
        if open {
            OpenResult::Ok(OutputHandle::new(name, TexpressoWriter{io: self.clone(), id, pos: 0}))
        } else {
            OpenResult::NotAvailable
        }
    }

    /// Open the standard output stream.
    fn output_open_stdout(&mut self) -> OpenResult<OutputHandle> {
        //return self.output_open_name("stdout");
        return OpenResult::NotAvailable
    }

    /// Open the named file for input.
    fn input_open_name(
        &mut self,
        name: &str,
        _status: &mut dyn StatusBackend,
    ) -> OpenResult<InputHandle> {
        let (id, open, gen) = {
            let mut io = self.borrow_mut();
            if name == io.last_passed_open {
                return OpenResult::NotAvailable
            } else {
                io.last_passed_open = name.to_string()
            }
            let id = io.alloc_id();
            let open = io.client.open(id, name, "r?");
            if !open { io.release_id(id); };
            (id, open, io.gen)
        };
        if open {
            let reader = TexpressoReader {
                io: self.clone(),
                id, gen,
                abs_pos: 0,
                buf: [0; 1024],
                buf_pos: 0,
                buf_len: 0,
                size: None,
            };
            OpenResult::Ok(InputHandle::new(name, reader, InputOrigin::Other))
        } else {
            OpenResult::NotAvailable
        }
    }

    fn input_open_format(
        &mut self,
        _name: &str,
        _status: &mut dyn StatusBackend,
    ) -> OpenResult<InputHandle> {
        OpenResult::NotAvailable
    }

    fn input_open_primary(&mut self, status: &mut dyn StatusBackend) -> OpenResult<InputHandle> {
        self.input_open_name("main.tex", status)
    }
}
