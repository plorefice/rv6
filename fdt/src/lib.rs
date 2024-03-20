#![cfg_attr(not(feature = "std"), no_std)]

use core::fmt;

use nom::{
    bytes::complete::{tag, take, take_while},
    combinator::map_res,
    multi::many0_count,
    number::complete::{be_u32, be_u64},
    IResult,
};

pub struct Fdt<'d> {
    hdr: Header,
    data: &'d [u8],
}

impl<'d> Fdt<'d> {
    pub fn from_bytes(fdt: &'d [u8]) -> Result<Self, FdtParseError> {
        let hdr = Header::from_bytes(fdt)?;

        if hdr.totalsize as usize != fdt.len() {
            return Err(FdtParseError::Truncated);
        }

        Ok(Self { hdr, data: fdt })
    }

    /// # Safety
    ///
    /// `fdt` must point to valid FDT data, in particular this function will use the `totalsize`
    /// field of the FDT header to retrieve the blob length. Moreover, the data must remain valid
    /// and not mutated for the duration of lifetime `'d`.
    pub unsafe fn from_raw_ptr(fdt: *const u8) -> Result<Self, FdtParseError<'d>> {
        let size = (fdt as *const u32).offset(1).read().to_be() as usize;
        let data = core::slice::from_raw_parts(fdt, size);
        Self::from_bytes(data)
    }

    pub fn size(&self) -> u32 {
        self.hdr.totalsize
    }

    pub fn boot_cpuid(&self) -> u32 {
        self.hdr.boot_cpuid_phys
    }

    pub fn reserved_memory_map(&self) -> impl Iterator<Item = Result<ReserveEntry, FdtParseError>> {
        self.data[self.hdr.off_mem_rsvmap as usize..]
            .chunks_exact(16)
            .map(ReserveEntry::from_bytes)
            .take_while(|res| match res {
                Ok(res) => !res.is_empty(),
                _ => false,
            })
    }

    pub fn root_node<'fdt>(&'fdt self) -> Result<Node<'d, 'fdt>, FdtParseError> {
        Node::from_bytes(
            self,
            0,
            None,
            &self.data[self.hdr.off_dt_struct as usize
                ..(self.hdr.off_dt_struct + self.hdr.size_dt_struct) as usize],
        )
    }

    pub fn find_by_path<'fdt>(
        &'fdt self,
        path: &str,
    ) -> Result<Option<Node<'d, 'fdt>>, FdtParseError> {
        let root = self.root_node()?;

        if path.is_empty() || path == "/" {
            return Ok(Some(root));
        }

        let mut node = root;

        for name in path[1..].split('/') {
            if let Some(child) = node.children().find(|n| n.identifier() == name) {
                node = child;
            } else {
                return Ok(None);
            }
        }

        Ok(Some(node))
    }

    pub fn find_compatible<'fdt>(
        &'fdt self,
        compatbile: &str,
    ) -> Result<Option<Node<'d, 'fdt>>, FdtParseError> {
        self.find(|n| {
            matches!(
                n.property::<StringList>("compatible")
                    .map(|mut c| c.any(|c| c == compatbile)),
                Some(true)
            )
        })
    }

    pub fn find<'fdt, F>(&'fdt self, f: F) -> Result<Option<Node<'d, 'fdt>>, FdtParseError>
    where
        F: Fn(&Node<'d, 'fdt>) -> bool + Copy,
    {
        self.root_node()?.find(f)
    }

    fn get_string(&self, off: u32) -> Option<&'d str> {
        let start = self.hdr.off_dt_strings + off;
        let len = self.data[start as usize..].iter().position(|&b| b == 0)?;

        let s = self.data.get(start as usize..start as usize + len)?;
        Some(core::str::from_utf8(s).expect("invalid utf-8 in string block"))
    }
}

#[derive(Debug, Clone, Copy)]
struct Header {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    _size_dt_strings: u32,
    size_dt_struct: u32,
}

impl Header {
    fn from_bytes(s: &[u8]) -> Result<Self, FdtParseError> {
        let (_, header) = Header::parse(s).map_err(FdtParseError::ParseError)?;

        if header.magic != 0xd00dfeed {
            return Err(FdtParseError::InvalidHeader);
        }

        if header.version != 17 || header.last_comp_version != 16 {
            return Err(FdtParseError::UnsupportedVersion(header.last_comp_version));
        }

        Ok(header)
    }

    fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, magic) = be_u32(input)?;
        let (input, totalsize) = be_u32(input)?;
        let (input, off_dt_struct) = be_u32(input)?;
        let (input, off_dt_strings) = be_u32(input)?;
        let (input, off_mem_rsvmap) = be_u32(input)?;
        let (input, version) = be_u32(input)?;
        let (input, last_comp_version) = be_u32(input)?;
        let (input, boot_cpuid_phys) = be_u32(input)?;
        let (input, size_dt_strings) = be_u32(input)?;
        let (input, size_dt_struct) = be_u32(input)?;

        Ok((
            input,
            Self {
                magic,
                totalsize,
                off_dt_struct,
                off_dt_strings,
                off_mem_rsvmap,
                version,
                last_comp_version,
                boot_cpuid_phys,
                _size_dt_strings: size_dt_strings,
                size_dt_struct,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReserveEntry {
    pub address: u64,
    pub size: u64,
}

impl ReserveEntry {
    fn from_bytes(s: &[u8]) -> Result<Self, FdtParseError> {
        Ok(Self::parse(s).map_err(FdtParseError::ParseError)?.1)
    }

    pub fn is_empty(&self) -> bool {
        self.address == 0 && self.size == 0
    }

    fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, address) = be_u64(input)?;
        let (input, size) = be_u64(input)?;

        Ok((input, Self { address, size }))
    }
}

#[derive(Clone)]
pub struct Node<'d, 'fdt> {
    fdt: &'fdt Fdt<'d>,
    off: usize,
    span: usize,
    name: &'d str,
    props: PropertyIter<'d, 'fdt>,
    children: NodeIter<'d, 'fdt>,
    parent_off: Option<usize>,
}

impl<'d, 'fdt> Node<'d, 'fdt> {
    fn from_bytes(
        fdt: &'fdt Fdt<'d>,
        off: usize,
        parent_off: Option<usize>,
        s: &'d [u8],
    ) -> Result<Self, FdtParseError<'d>> {
        Ok(Node::parse(fdt, off, parent_off, s)
            .map_err(FdtParseError::ParseError)?
            .1)
    }

    pub fn identifier(&self) -> &str {
        self.name
    }

    pub fn name(&self) -> &str {
        self.name
            .split_once('@')
            .map(|(name, _)| name)
            .unwrap_or(self.name)
    }

    pub fn address(&self) -> Option<&str> {
        self.name.split_once('@').map(|(_, addr)| addr)
    }

    pub fn properties(&self) -> impl Iterator<Item = Property<'d>> + 'fdt {
        self.props.clone()
    }

    pub fn property<T: 'd>(&self, name: &str) -> Option<T>
    where
        T: PropValue<'d>,
    {
        self.properties()
            .find(|p| p.name() == Some(name))
            .and_then(|p| p.value())
    }

    pub fn children(&self) -> impl Iterator<Item = Node<'d, 'fdt>> {
        self.children.clone()
    }

    pub fn parent(&self) -> Option<Node<'d, 'fdt>> {
        let parent_off = self.parent_off?;
        self.fdt.find(|n| n.off == parent_off).ok().flatten()
    }

    fn find<F>(&self, f: F) -> Result<Option<Node<'d, 'fdt>>, FdtParseError<'d>>
    where
        F: Fn(&Node<'d, 'fdt>) -> bool + Copy,
    {
        if f(self) {
            return Ok(Some(self.clone()));
        }

        for child in self.children() {
            if let Some(n) = child.find(f)? {
                return Ok(Some(n));
            }
        }

        Ok(None)
    }

    fn span(&self) -> usize {
        self.span
    }

    fn parse(
        fdt: &'fdt Fdt<'d>,
        off: usize,
        parent_off: Option<usize>,
        input: &'d [u8],
    ) -> IResult<&'d [u8], Self> {
        let start = input;

        // Skip FDT_NOP tokens
        let (input, _) = many0_count(tag(&[0, 0, 0, 4]))(input)?;

        // FDT_BEGIN_NODE
        let (input, _) = tag(&[0, 0, 0, 1])(input)?;

        // NUL terminated name
        let (input, name) = map_res(take_while(|c| c != 0), core::str::from_utf8)(input)?;
        let (input, _) = tag(&[0])(input)?;

        // Padded to 4 bytes
        let n = start.len() - input.len();
        let next = (n + 3) & !3;
        let (input, _) = take(next - n)(input)?;

        // Properties
        let props = PropertyIter::new(fdt, input);
        let (input, _) = take(props.clone().span())(input)?;

        // Children
        let children = NodeIter::new(fdt, off + start.len() - input.len(), off, input);
        let (input, _) = take(children.clone().span())(input)?;

        // Skip FDT_NOP tokens
        let (input, _) = many0_count(tag(&[0, 0, 0, 4]))(input)?;

        // FDT_END
        let (input, _) = tag(&[0, 0, 0, 2])(input)?;

        let span = start.len() - input.len();

        Ok((
            input,
            Self {
                fdt,
                off,
                span,
                name,
                props,
                children,
                parent_off,
            },
        ))
    }
}

impl<'d> fmt::Debug for Node<'d, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.off)
            .field("offset", &self.off)
            .field("span", &self.span)
            .field("name", &self.name)
            .field("parent", &self.parent_off)
            .finish()
    }
}

#[derive(Clone)]
pub struct PropertyIter<'d, 'fdt> {
    fdt: &'fdt Fdt<'d>,
    data: &'d [u8],
}

impl<'d, 'fdt> PropertyIter<'d, 'fdt> {
    fn new(fdt: &'fdt Fdt<'d>, data: &'d [u8]) -> Self {
        Self { fdt, data }
    }

    fn span(self) -> usize {
        self.map(|prop| prop.span()).sum()
    }
}

impl<'d> Iterator for PropertyIter<'d, '_> {
    type Item = Property<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, prop) = Property::parse(self.fdt, self.data).ok()?;
        self.data = data;
        Some(prop)
    }
}

#[derive(Debug, Clone)]
pub struct Property<'d> {
    span: usize,
    name: Option<&'d str>,
    data: &'d [u8],
}

impl<'d> Property<'d> {
    pub fn name(&self) -> Option<&'d str> {
        self.name
    }

    pub fn raw_value(&self) -> &'d [u8] {
        self.data
    }

    pub fn value<T>(&self) -> Option<T>
    where
        T: PropValue<'d> + 'd,
    {
        T::parse(self.data).map(|(_, t)| t)
    }

    fn span(&self) -> usize {
        self.span
    }

    fn parse<'fdt>(fdt: &'fdt Fdt<'d>, input: &'d [u8]) -> IResult<&'d [u8], Self> {
        let start = input;

        // Skip FDT_NOP tokens
        let (input, _) = many0_count(tag(&[0, 0, 0, 4]))(input)?;

        // Skip FDT_PROP token
        let (input, _) = tag(&[0, 0, 0, 3])(input)?;

        // Property length and name string offset
        let (input, len) = be_u32(input)?;
        let (input, name_off) = be_u32(input)?;

        // Skip data
        let (input, data) = take(len)(input)?;

        // Skip padding
        let n = start.len() - input.len();
        let span = (n + 3) & !3;
        let (input, _) = take(span - n)(input)?;

        Ok((
            input,
            Self {
                span,
                name: fdt.get_string(name_off),
                data,
            },
        ))
    }
}

#[derive(Clone)]
struct NodeIter<'d, 'fdt> {
    fdt: &'fdt Fdt<'d>,
    data: &'d [u8],
    offset: usize,
    parent_off: usize,
}

impl<'d, 'fdt> NodeIter<'d, 'fdt> {
    fn new(fdt: &'fdt Fdt<'d>, offset: usize, parent_off: usize, data: &'d [u8]) -> Self {
        Self {
            fdt,
            data,
            offset,
            parent_off,
        }
    }

    fn span(self) -> usize {
        self.map(|node| node.span()).sum()
    }
}

impl<'d, 'fdt> Iterator for NodeIter<'d, 'fdt> {
    type Item = Node<'d, 'fdt>;

    fn next(&mut self) -> Option<Self::Item> {
        let (rest, node) =
            Node::parse(self.fdt, self.offset, Some(self.parent_off), self.data).ok()?;
        self.offset += self.data.len() - rest.len();
        self.data = rest;
        Some(node)
    }
}

pub struct PropEncodedArray<'d, T>
where
    T: PropValue<'d>,
{
    data: &'d [u8],
    _marker: core::marker::PhantomData<T>,
}

impl<'d, T: 'd> Iterator for PropEncodedArray<'d, T>
where
    T: PropValue<'d>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, t) = T::parse(self.data)?;
        self.data = data;
        Some(t)
    }
}

pub struct StringList<'d> {
    data: &'d [u8],
}

impl<'d> Iterator for StringList<'d> {
    type Item = &'d str;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, s) = Self::Item::parse(self.data)?;
        self.data = data;
        Some(s)
    }
}

pub trait PropValue<'d>: Sized {
    fn parse(data: &'d [u8]) -> Option<(&'d [u8], Self)>
    where
        Self: 'd;
}

impl PropValue<'_> for u32 {
    fn parse(data: &[u8]) -> Option<(&[u8], Self)> {
        if data.len() < 4 {
            return None;
        }

        let (data, rest) = data.split_at(4);
        (rest, u32::from_be_bytes(data.try_into().unwrap())).into()
    }
}

impl PropValue<'_> for u64 {
    fn parse(data: &[u8]) -> Option<(&[u8], Self)> {
        let (data, hi) = u32::parse(data)?;
        let (data, lo) = u32::parse(data)?;
        (data, (hi as u64) << 32 | lo as u64).into()
    }
}

impl<'d> PropValue<'d> for &'d str {
    fn parse(data: &'d [u8]) -> Option<(&'d [u8], Self)> {
        let n = data.iter().position(|&b| b == 0)?;
        let (data, rest) = data.split_at(n);
        (&rest[1..], core::str::from_utf8(data).ok()?).into()
    }
}

impl<'d, T: 'd, U: 'd> PropValue<'d> for (T, U)
where
    T: PropValue<'d>,
    U: PropValue<'d>,
{
    fn parse(data: &'d [u8]) -> Option<(&'d [u8], Self)> {
        let (data, t) = T::parse(data)?;
        let (data, u) = U::parse(data)?;
        (data, (t, u)).into()
    }
}

impl<'d, T: 'd> PropValue<'d> for PropEncodedArray<'d, T>
where
    T: PropValue<'d>,
{
    fn parse(data: &'d [u8]) -> Option<(&'d [u8], Self)> {
        Some((
            &[], // prop-encoded-arrays consume all data
            PropEncodedArray {
                data,
                _marker: core::marker::PhantomData,
            },
        ))
    }
}

impl<'d> PropValue<'d> for StringList<'d> {
    fn parse(data: &'d [u8]) -> Option<(&'d [u8], Self)> {
        Some((
            &[], // string-lists consume all data
            StringList { data },
        ))
    }
}

#[derive(Debug)]
pub enum FdtParseError<'e> {
    Truncated,
    InvalidHeader,
    UnsupportedVersion(u32),
    ParseError(nom::Err<nom::error::Error<&'e [u8]>>),
}
