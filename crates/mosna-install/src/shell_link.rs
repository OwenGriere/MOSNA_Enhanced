//! Writing a Windows shell link (`.lnk`).
//!
//! # Why by hand
//!
//! The published crates for this either wrap COM — unavailable when
//! cross-compiling, and untestable from Linux — or only compile on Windows,
//! which would mean the installer's Windows path could not be exercised until
//! it was already in a user's hands. The format is documented (MS-SHLLINK) and
//! the part needed here is small, so it is written directly and its bytes are
//! asserted against the specification.
//!
//! What is produced is a link with `LinkInfo`, a display name, a working
//! directory and an icon: enough for Explorer to show it and launch it.

use std::path::Path;

/// `HeaderSize`, fixed by the specification.
const HEADER_SIZE: u32 = 0x0000_004C;

/// `LinkCLSID`: 00021401-0000-0000-C000-000000000046, in little-endian layout.
const LINK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

// LinkFlags.
const HAS_LINK_INFO: u32 = 0x0000_0002;
const HAS_NAME: u32 = 0x0000_0004;
const HAS_WORKING_DIR: u32 = 0x0000_0010;
const HAS_ICON_LOCATION: u32 = 0x0000_0040;
const IS_UNICODE: u32 = 0x0000_0080;

/// `FILE_ATTRIBUTE_NORMAL`.
const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
/// `SW_SHOWNORMAL`.
const SHOW_NORMAL: u32 = 1;
/// `DRIVE_FIXED`.
const DRIVE_FIXED: u32 = 3;

/// `LinkInfoHeaderSize` for the layout that carries the Unicode path fields.
///
/// The ANSI-only layout (0x1C) mangles any path containing a character outside
/// the code page — a user account named `José` would produce a link to
/// nowhere. The larger header adds the Unicode copies.
const LINK_INFO_HEADER_SIZE: u32 = 0x0000_0024;

/// A shell link to build.
#[derive(Debug, Clone)]
pub struct ShellLink {
    target: String,
    name: Option<String>,
    working_dir: Option<String>,
    icon: Option<String>,
}

impl ShellLink {
    /// A link pointing at `target`, which must be an absolute Windows path.
    pub fn new(target: impl AsRef<str>) -> Self {
        Self {
            target: target.as_ref().to_string(),
            name: None,
            working_dir: None,
            icon: None,
        }
    }

    /// The description Explorer shows.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The directory the target starts in.
    pub fn with_working_dir(mut self, directory: impl Into<String>) -> Self {
        self.working_dir = Some(directory.into());
        self
    }

    /// The icon file.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Serialise the link.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut flags = HAS_LINK_INFO | IS_UNICODE;
        if self.name.is_some() {
            flags |= HAS_NAME;
        }
        if self.working_dir.is_some() {
            flags |= HAS_WORKING_DIR;
        }
        if self.icon.is_some() {
            flags |= HAS_ICON_LOCATION;
        }

        let mut bytes = Vec::with_capacity(512);

        // ShellLinkHeader.
        bytes.extend(HEADER_SIZE.to_le_bytes());
        bytes.extend(LINK_CLSID);
        bytes.extend(flags.to_le_bytes());
        bytes.extend(FILE_ATTRIBUTE_NORMAL.to_le_bytes());
        // The three timestamps are optional; zero means "unknown", which
        // Explorer accepts and which keeps the output reproducible.
        bytes.extend([0u8; 24]);
        bytes.extend(0u32.to_le_bytes()); // FileSize
        bytes.extend(0i32.to_le_bytes()); // IconIndex
        bytes.extend(SHOW_NORMAL.to_le_bytes());
        bytes.extend(0u16.to_le_bytes()); // HotKey
        bytes.extend([0u8; 10]); // Reserved, Reserved2, Reserved3
        debug_assert_eq!(bytes.len(), HEADER_SIZE as usize);

        bytes.extend(self.link_info());

        // StringData, in the order the specification fixes.
        for value in [&self.name, &self.working_dir, &self.icon]
            .into_iter()
            .flatten()
        {
            bytes.extend(string_data(value));
        }

        // TerminalBlock: an ExtraData section of size zero ends the file.
        bytes.extend(0u32.to_le_bytes());
        bytes
    }

    /// Write the link to `path`.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// The `LinkInfo` structure, naming the target's local path.
    fn link_info(&self) -> Vec<u8> {
        let base_ansi = to_ansi(&self.target);
        let base_wide = to_utf16_nul(&self.target);

        // VolumeID: an empty label is enough; Explorer resolves the drive from
        // the path itself.
        let volume_id_size: u32 = 4 + 4 + 4 + 4 + 1;
        let volume_id_offset = LINK_INFO_HEADER_SIZE;
        let local_base_path_offset = volume_id_offset + volume_id_size;
        let common_path_suffix_offset = local_base_path_offset + base_ansi.len() as u32;
        // The suffix is a single terminator, so the Unicode block starts after
        // it.
        let local_base_path_offset_unicode = common_path_suffix_offset + 1;
        let common_path_suffix_offset_unicode =
            local_base_path_offset_unicode + base_wide.len() as u32;
        let total = common_path_suffix_offset_unicode + 2;

        let mut info = Vec::with_capacity(total as usize);
        info.extend(total.to_le_bytes());
        info.extend(LINK_INFO_HEADER_SIZE.to_le_bytes());
        info.extend(1u32.to_le_bytes()); // VolumeIDAndLocalBasePath
        info.extend(volume_id_offset.to_le_bytes());
        info.extend(local_base_path_offset.to_le_bytes());
        info.extend(0u32.to_le_bytes()); // CommonNetworkRelativeLinkOffset
        info.extend(common_path_suffix_offset.to_le_bytes());
        info.extend(local_base_path_offset_unicode.to_le_bytes());
        info.extend(common_path_suffix_offset_unicode.to_le_bytes());
        debug_assert_eq!(info.len(), LINK_INFO_HEADER_SIZE as usize);

        // VolumeID.
        info.extend(volume_id_size.to_le_bytes());
        info.extend(DRIVE_FIXED.to_le_bytes());
        info.extend(0u32.to_le_bytes()); // DriveSerialNumber
        info.extend(0x10u32.to_le_bytes()); // VolumeLabelOffset
        info.push(0); // an empty label

        info.extend(base_ansi);
        info.push(0); // CommonPathSuffix
        info.extend(base_wide);
        info.extend([0, 0]); // CommonPathSuffixUnicode

        debug_assert_eq!(info.len(), total as usize);
        info
    }
}

/// A `StringData` item: a character count, then UTF-16 without a terminator.
fn string_data(value: &str) -> Vec<u8> {
    let encoded: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(2 + encoded.len() * 2);
    bytes.extend((encoded.len() as u16).to_le_bytes());
    for unit in encoded {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

/// The path as bytes for the ANSI field, with its terminator.
///
/// Characters outside ASCII cannot be represented here; they are replaced so
/// the field stays well formed. The Unicode copy alongside carries the real
/// path, and that is what a modern Explorer reads.
fn to_ansi(value: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = value
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    bytes.push(0);
    bytes
}

/// The path as UTF-16 with its terminator.
fn to_utf16_nul(value: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend([0, 0]);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> ShellLink {
        ShellLink::new(r"C:\Programs\MOSNA\bin\mosna-gui.exe")
            .with_name("MOSNA")
            .with_working_dir(r"C:\Programs\MOSNA\bin")
            .with_icon(r"C:\Programs\MOSNA\share\mosna\mosna.ico")
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn the_header_matches_the_specification() {
        let bytes = link().to_bytes();
        assert_eq!(read_u32(&bytes, 0), HEADER_SIZE);
        assert_eq!(&bytes[4..20], &LINK_CLSID);
        assert_eq!(read_u32(&bytes, 24), FILE_ATTRIBUTE_NORMAL);
        assert_eq!(read_u32(&bytes, 60), SHOW_NORMAL);
    }

    #[test]
    fn the_flags_describe_what_is_present() {
        let flags = read_u32(&link().to_bytes(), 20);
        assert!(flags & HAS_LINK_INFO != 0);
        assert!(flags & HAS_NAME != 0);
        assert!(flags & HAS_WORKING_DIR != 0);
        assert!(flags & HAS_ICON_LOCATION != 0);
        assert!(flags & IS_UNICODE != 0);
    }

    /// A link without an icon must not claim to have one, or Explorer reads
    /// past the end of the string data.
    #[test]
    fn absent_fields_clear_their_flag() {
        let bare = ShellLink::new(r"C:\a.exe").to_bytes();
        let flags = read_u32(&bare, 20);
        assert!(flags & HAS_NAME == 0);
        assert!(flags & HAS_WORKING_DIR == 0);
        assert!(flags & HAS_ICON_LOCATION == 0);
        assert!(flags & HAS_LINK_INFO != 0);
    }

    #[test]
    fn the_link_info_offsets_are_self_consistent() {
        let bytes = link().to_bytes();
        let base = HEADER_SIZE as usize;

        let size = read_u32(&bytes, base) as usize;
        assert_eq!(read_u32(&bytes, base + 4), LINK_INFO_HEADER_SIZE);
        assert_eq!(read_u32(&bytes, base + 8), 1, "VolumeIDAndLocalBasePath");

        // Every offset must land inside the structure.
        for field in [12, 16, 24, 28, 32] {
            let offset = read_u32(&bytes, base + field) as usize;
            assert!(
                offset < size,
                "offset at +{field} is {offset}, size is {size}"
            );
        }
    }

    #[test]
    fn the_target_appears_as_ansi_and_as_unicode() {
        let bytes = link().to_bytes();
        let target = r"C:\Programs\MOSNA\bin\mosna-gui.exe";

        assert!(
            bytes.windows(target.len()).any(|w| w == target.as_bytes()),
            "the ANSI path is missing"
        );
        let wide: Vec<u8> = target.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(
            bytes.windows(wide.len()).any(|w| w == wide),
            "the Unicode path is missing"
        );
    }

    /// A path with a non-ASCII character — an accented user name — must still
    /// produce a link whose Unicode field holds the real path.
    #[test]
    fn a_non_ascii_path_survives_in_the_unicode_field() {
        let target = r"C:\Users\José\MOSNA\mosna-gui.exe";
        let bytes = ShellLink::new(target).to_bytes();

        let wide: Vec<u8> = target.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(
            bytes.windows(wide.len()).any(|w| w == wide),
            "the Unicode path was mangled"
        );
    }

    #[test]
    fn the_file_ends_with_the_terminal_block() {
        let bytes = link().to_bytes();
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn the_name_is_stored_as_utf16_with_a_count() {
        let bytes = ShellLink::new(r"C:\a.exe").with_name("MOSNA").to_bytes();
        let wide: Vec<u8> = "MOSNA".encode_utf16().flat_map(u16::to_le_bytes).collect();

        let position = bytes
            .windows(wide.len())
            .position(|w| w == wide)
            .expect("the name is missing");
        // The two bytes before it are the character count.
        let count = u16::from_le_bytes(bytes[position - 2..position].try_into().unwrap());
        assert_eq!(count, 5);
    }

    #[test]
    fn writing_produces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("MOSNA.lnk");
        link().write(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), link().to_bytes());
    }

    #[test]
    fn the_output_is_reproducible() {
        assert_eq!(link().to_bytes(), link().to_bytes());
    }
}
