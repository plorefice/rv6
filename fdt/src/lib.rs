#![cfg_attr(not(feature = "std"), no_std)]

use core::fmt;

use nom::{
    bytes::complete::{tag, take, take_while},
    combinator::map_res,
    multi::many0_count,
    number::complete::{be_u32, be_u64},
    IResult,
};

#[derive(Clone)]
pub struct Fdt<'a> {
    hdr: Header,
    data: &'a [u8],
}

impl<'a> Fdt<'a> {
    pub fn from_bytes(fdt: &'a [u8]) -> Result<Self, FdtParseError> {
        let hdr = Header::from_bytes(fdt)?;

        if hdr.totalsize as usize != fdt.len() {
            return Err(FdtParseError::Truncated);
        }

        Ok(Self { hdr, data: fdt })
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

    pub fn root_node(&'a self) -> Result<Node<'a>, FdtParseError> {
        Node::from_bytes(
            self,
            &self.data[self.hdr.off_dt_struct as usize
                ..(self.hdr.off_dt_struct + self.hdr.size_dt_struct) as usize],
        )
    }

    pub fn find_by_path(&'a self, path: &str) -> Result<Option<Node<'a>>, FdtParseError> {
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

    fn get_string(&self, off: u32) -> Option<&'a str> {
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
pub struct Node<'a> {
    span: usize,
    name: &'a str,
    props: PropertyIter<'a>,
    children: NodeIter<'a>,
}

impl<'a> Node<'a> {
    fn from_bytes(fdt: &'a Fdt<'a>, s: &'a [u8]) -> Result<Self, FdtParseError<'a>> {
        Ok(Node::parse(fdt, s).map_err(FdtParseError::ParseError)?.1)
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

    pub fn properties(&self) -> impl Iterator<Item = Property<'a>> {
        self.props.clone()
    }

    pub fn property<T: 'a>(&self, name: &str) -> Option<T>
    where
        T: PropValue<'a>,
    {
        self.properties()
            .find(|p| p.name() == Some(name))
            .and_then(|p| p.value())
    }

    pub fn children(&self) -> impl Iterator<Item = Node<'a>> {
        self.children.clone()
    }

    fn span(&self) -> usize {
        self.span
    }

    fn parse(fdt: &'a Fdt<'a>, input: &'a [u8]) -> IResult<&'a [u8], Self> {
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
        let children = NodeIter::new(fdt, input);
        let (input, _) = take(children.clone().span())(input)?;

        // Skip FDT_NOP tokens
        let (input, _) = many0_count(tag(&[0, 0, 0, 4]))(input)?;

        // FDT_END
        let (input, _) = tag(&[0, 0, 0, 2])(input)?;

        let span = start.len() - input.len();

        Ok((
            input,
            Self {
                span,
                name,
                props,
                children,
            },
        ))
    }
}

impl<'a> fmt::Debug for Node<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("span", &self.span)
            .field("name", &self.name)
            .finish()
    }
}

#[derive(Clone)]
pub struct PropertyIter<'a> {
    fdt: &'a Fdt<'a>,
    data: &'a [u8],
}

impl<'a> PropertyIter<'a> {
    fn new(fdt: &'a Fdt<'a>, data: &'a [u8]) -> Self {
        Self { fdt, data }
    }

    fn span(self) -> usize {
        self.map(|prop| prop.span()).sum()
    }
}

impl<'a> Iterator for PropertyIter<'a> {
    type Item = Property<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, prop) = Property::parse(self.fdt, self.data).ok()?;
        self.data = data;
        Some(prop)
    }
}

#[derive(Debug, Clone)]
pub struct Property<'a> {
    span: usize,
    name: Option<&'a str>,
    data: &'a [u8],
}

impl<'a> Property<'a> {
    pub fn name(&self) -> Option<&'a str> {
        self.name
    }

    pub fn raw_value(&self) -> &'a [u8] {
        self.data
    }

    pub fn value<T>(&self) -> Option<T>
    where
        T: PropValue<'a> + 'a,
    {
        T::parse(self.data).map(|(_, t)| t)
    }

    fn span(&self) -> usize {
        self.span
    }

    fn parse(fdt: &'a Fdt<'a>, input: &'a [u8]) -> IResult<&'a [u8], Self> {
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
struct NodeIter<'a> {
    fdt: &'a Fdt<'a>,
    data: &'a [u8],
}

impl<'a> NodeIter<'a> {
    fn new(fdt: &'a Fdt<'a>, data: &'a [u8]) -> Self {
        Self { fdt, data }
    }

    fn span(self) -> usize {
        self.map(|node| node.span()).sum()
    }
}

impl<'a> Iterator for NodeIter<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, node) = Node::parse(self.fdt, self.data).ok()?;
        self.data = data;
        Some(node)
    }
}

pub struct PropEncodedArray<'v, T>
where
    T: PropValue<'v>,
{
    data: &'v [u8],
    _marker: core::marker::PhantomData<T>,
}

impl<'v, T: 'v> Iterator for PropEncodedArray<'v, T>
where
    T: PropValue<'v>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, t) = T::parse(self.data)?;
        self.data = data;
        Some(t)
    }
}

pub struct StringList<'v> {
    data: &'v [u8],
}

impl<'v> Iterator for StringList<'v> {
    type Item = &'v str;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, s) = Self::Item::parse(self.data)?;
        self.data = data;
        Some(s)
    }
}

pub trait PropValue<'v>: Sized {
    fn parse(data: &'v [u8]) -> Option<(&'v [u8], Self)>
    where
        Self: 'v;
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

impl<'v> PropValue<'v> for &'v str {
    fn parse(data: &'v [u8]) -> Option<(&'v [u8], Self)> {
        let n = data.iter().position(|&b| b == 0)?;
        let (data, rest) = data.split_at(n);
        (&rest[1..], core::str::from_utf8(data).ok()?).into()
    }
}

impl<'v, T: 'v, U: 'v> PropValue<'v> for (T, U)
where
    T: PropValue<'v>,
    U: PropValue<'v>,
{
    fn parse(data: &'v [u8]) -> Option<(&'v [u8], Self)> {
        let (data, t) = T::parse(data)?;
        let (data, u) = U::parse(data)?;
        (data, (t, u)).into()
    }
}

impl<'v, T: 'v> PropValue<'v> for PropEncodedArray<'v, T>
where
    T: PropValue<'v>,
{
    fn parse(data: &'v [u8]) -> Option<(&'v [u8], Self)> {
        Some((
            &[], // prop-encoded-arrays consume all data
            PropEncodedArray {
                data,
                _marker: core::marker::PhantomData,
            },
        ))
    }
}

impl<'v> PropValue<'v> for StringList<'v> {
    fn parse(data: &'v [u8]) -> Option<(&'v [u8], Self)> {
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
