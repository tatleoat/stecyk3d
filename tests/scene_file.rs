// SPDX-License-Identifier: MIT
// Tests of the composite-file reader on files built to the layouts.
// (Tests)

use stecyk3d::scene_file::{Container, ContainerError, Flavour, ResourceIndex};

/// Builds composite files: a header, then pieces appended at the end.
struct Builder {
    file: Vec<u8>,
}

impl Builder {
    fn new() -> Self {
        let mut file = vec![0u8; 0x300];
        file[..4].copy_from_slice(b"CCmF");
        file[0x200..0x200 + 23].copy_from_slice(b"Resource Composite File");
        Builder { file }
    }

    fn put(&mut self, at: usize, v: u32) {
        self.file[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Appends `data` as a chain of pieces of at most `chunk` bytes each; returns the first offset.
    fn chain(&mut self, data: &[u8], chunk: usize) -> u32 {
        if data.is_empty() {
            return 0;
        }
        let mut first = 0;
        let mut prev_next: Option<usize> = None;
        for part in data.chunks(chunk) {
            let at = self.file.len();
            if let Some(p) = prev_next {
                self.put(p, at as u32);
            } else {
                first = at as u32;
            }
            self.file.extend_from_slice(&(12 + part.len() as u32 + 4).to_be_bytes()); // room, head included
            self.file.extend_from_slice(&(part.len() as u32).to_be_bytes()); // used
            self.file.extend_from_slice(&0u32.to_be_bytes()); // next
            self.file.extend_from_slice(part);
            self.file.extend_from_slice(&[0xee; 4]); // spare room
            prev_next = Some(at + 8);
        }
        first
    }

    /// Writes entries (None: not in use) and the directory.
    fn finish(mut self, entries: &[Option<Vec<u8>>]) -> Vec<u8> {
        let mut dir = Vec::new();
        for e in entries {
            match e {
                None => dir.extend_from_slice(&[0u8; 12]),
                Some(bytes) => {
                    let first = self.chain(bytes, 7);
                    dir.extend_from_slice(&[1, 0, 0, 0]);
                    dir.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                    dir.extend_from_slice(&first.to_be_bytes());
                }
            }
        }
        let first = self.chain(&dir, 40);
        self.put(0x24c, dir.len() as u32);
        self.put(0x250, first);
        self.put(0x260, entries.len() as u32);
        self.file
    }
}

fn simple() -> Vec<u8> {
    Builder::new().finish(&[Some(b"hello, pieces".to_vec()), None, Some(Vec::new())])
}

#[test]
fn reads_entries_across_pieces() {
    let c = Container::parse(&simple(), Flavour::Bryce3d).unwrap();
    assert_eq!(c.entries.len(), 3);
    assert_eq!(c.entry(0), Some(&b"hello, pieces"[..]));
    assert!(!c.entries[1].in_use);
    assert_eq!(c.entry(2), Some(&b""[..]));
    assert_eq!(c.entry(3), None);
}

#[test]
fn rejects_missing_magic_and_short_files() {
    let mut f = simple();
    f[0] = b'X';
    assert_eq!(Container::parse(&f, Flavour::Bryce3d), Err(ContainerError::NoIdentifyingText));
    let f = simple();
    assert!(matches!(Container::parse(&f[..0x26b], Flavour::Bryce3d), Err(ContainerError::TooShort { .. })));
}

#[test]
fn bryce5_second_form_of_the_identifying_text() {
    let with_head = |head: &[u8]| {
        let mut f = simple();
        f[..4].fill(0);
        f[..head.len()].copy_from_slice(head);
        f
    };
    let mut good = vec![b' '; 94];
    good.extend_from_slice(b"CCmF");
    let f = with_head(&good);
    assert!(Container::parse(&f, Flavour::Bryce5).is_ok());
    assert_eq!(Container::parse(&f, Flavour::Bryce3d), Err(ContainerError::NoIdentifyingText));

    let mut short = vec![b' '; 93];
    short.extend_from_slice(b"CCmF");
    assert_eq!(Container::parse(&with_head(&short), Flavour::Bryce5), Err(ContainerError::NoIdentifyingText));

    let mut zero_first = vec![0u8];
    zero_first.extend(vec![b' '; 93]);
    zero_first.extend_from_slice(b"CCmF");
    assert_eq!(Container::parse(&with_head(&zero_first), Flavour::Bryce5), Err(ContainerError::NoIdentifyingText));
}

#[test]
fn rejects_format_name_and_header_words() {
    let mut f = simple();
    f[0x201] = b'X';
    assert_eq!(Container::parse(&f, Flavour::Bryce3d), Err(ContainerError::WrongFormatName));
    let mut f = simple();
    f[0x227] = 1;
    assert!(matches!(
        Container::parse(&f, Flavour::Bryce3d),
        Err(ContainerError::NonZeroHeaderWord { offset: 0x224, value: 1 })
    ));
}

#[test]
fn rejects_directory_that_does_not_fit() {
    let mut f = simple();
    f[0x263] = 4; // four entries claimed, room for three
    assert!(matches!(Container::parse(&f, Flavour::Bryce3d), Err(ContainerError::DirectoryDoesNotFit { .. })));
}

#[test]
fn rejects_piece_past_end_and_endless_chain() {
    let f = simple();
    let first = u32::from_be_bytes(f[0x250..0x254].try_into().unwrap()) as usize;
    let mut g = f.clone();
    g[first + 4..first + 8].copy_from_slice(&(u32::MAX).to_be_bytes());
    assert!(matches!(Container::parse(&g, Flavour::Bryce3d), Err(ContainerError::PiecePastEnd { .. })));
    let mut g = f.clone();
    g[first + 8..first + 12].copy_from_slice(&(first as u32).to_be_bytes());
    assert!(matches!(Container::parse(&g, Flavour::Bryce3d), Err(ContainerError::ChainDoesNotEnd { .. })));
}

#[test]
fn rejects_entry_size_mismatch() {
    let mut f = simple();
    let first = u32::from_be_bytes(f[0x250..0x254].try_into().unwrap()) as usize;
    // Entry 0's size field is at directory bytes 4..8, inside the first directory piece.
    f[first + 12 + 7] = 99;
    assert!(matches!(
        Container::parse(&f, Flavour::Bryce3d),
        Err(ContainerError::EntrySizeMismatch { entry: 0, size: 99, .. })
    ));
}

fn class_record(tag: &[u8], table: u32, descriptor: u32) -> Vec<u8> {
    let mut r = vec![0u8; 32];
    r[..tag.len()].copy_from_slice(tag);
    r[16..20].copy_from_slice(&table.to_be_bytes());
    r[20..24].copy_from_slice(&descriptor.to_be_bytes());
    r
}

fn resource_row(identity: u32, e1: u32, e2: u32, name: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; 60];
    r[0..4].copy_from_slice(&identity.to_be_bytes());
    r[4..8].copy_from_slice(&e1.to_be_bytes());
    r[8..12].copy_from_slice(&e2.to_be_bytes());
    r[12..12 + name.len()].copy_from_slice(name);
    r
}

#[test]
fn reads_resource_index() {
    let mut entries: Vec<Option<Vec<u8>>> = vec![None; 20];
    let mut list = class_record(b"Abcd\0junk", 17, 18);
    list.extend(class_record(b"", 0, 0)); // no class
    entries[16] = Some(list);
    let mut table = resource_row(7, 1, 2, b"First");
    table.extend(resource_row(9, 3, 4, b"Second\0rest"));
    table.extend(resource_row(11, 5, 6, b"room only")); // beyond the count
    entries[17] = Some(table);
    let mut desc = vec![0u8; 32];
    desc[12..16].copy_from_slice(&2u32.to_be_bytes());
    entries[18] = Some(desc);
    let f = Builder::new().finish(&entries);

    let c = Container::parse(&f, Flavour::Bryce3d).unwrap();
    let idx = ResourceIndex::read(&c).unwrap();
    assert_eq!(idx.classes.len(), 1);
    let class = idx.class(b"Abcd").unwrap();
    assert_eq!(class.resources.len(), 2);
    assert_eq!(class.resources[0].identity, 7);
    assert_eq!(class.resources[0].entries, [1, 2]);
    assert_eq!(class.resources[1].name, b"Second");
}
