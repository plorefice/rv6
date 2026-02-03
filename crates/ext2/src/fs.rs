use crate::io::{Read, Seek, Write};

pub struct FileSystem<IO: Read + Write + Seek> {
    disk: IO,
}

pub trait IntoStorage<T: Read + Write + Seek> {
    fn into_storage(self) -> T;
}

impl<T: Read + Write + Seek> IntoStorage<T> for T {
    fn into_storage(self) -> T {
        self
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Read + std::io::Write + std::io::Seek> IntoStorage<crate::io::StdIoWrapper<T>>
    for T
{
    fn into_storage(self) -> crate::io::StdIoWrapper<T> {
        crate::io::StdIoWrapper::from(self)
    }
}

impl<IO: Read + Write + Seek> FileSystem<IO> {
    pub fn new<T: IntoStorage<IO>>(disk: T) -> Self {
        Self {
            disk: disk.into_storage(),
        }
    }
}
