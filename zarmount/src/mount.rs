use clap::Parser;
use fuser::{
    Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
    LockOwner, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use zarlib::{
    TocEntry,
    operations::{PasswordSet, PreparedExtraction, parse_commit_id},
};
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TTL: Duration = Duration::from_secs(1);
const ROOT: u64 = 1;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[derive(Parser)]
#[command(
    version,
    about = "Mount a ZAR archive as a read-only Linux FUSE filesystem"
)]
pub struct Options {
    /// Archive to mount.
    archive: PathBuf,
    /// Existing empty directory to mount on.
    mountpoint: PathBuf,
    /// Select the archive state through this eight-digit commit ID.
    #[arg(long)]
    to: Option<String>,
    /// File containing the default password (one optional trailing newline is removed).
    #[arg(long)]
    password_file: Option<PathBuf>,
    /// Commit-specific password file, as COMMIT_ID=PATH. May be repeated.
    #[arg(long, value_name = "COMMIT_ID=PATH")]
    password_for: Vec<String>,
}

fn password_file(path: &Path) -> Result<Vec<u8>> {
    let mut password = Zeroizing::new(fs::read(path)?);
    if password.last() == Some(&b'\n') {
        password.pop();
        if password.last() == Some(&b'\r') {
            password.pop();
        }
    }
    if password.is_empty() {
        return Err(format!("password file {} is empty", path.display()).into());
    }
    Ok(std::mem::take(&mut *password))
}

fn passwords(options: &Options) -> Result<PasswordSet> {
    let default = options
        .password_file
        .as_deref()
        .map(password_file)
        .transpose()?;
    let mut passwords = PasswordSet::new(default);
    let mut ids = BTreeSet::new();
    for spec in &options.password_for {
        let (id, path) = spec
            .split_once('=')
            .ok_or("--password-for must be COMMIT_ID=PATH")?;
        if path.is_empty() {
            return Err("--password-for requires a password file path".into());
        }
        let commit = parse_commit_id(id)?;
        if !ids.insert(commit) {
            return Err(format!("password for commit {id} specified more than once").into());
        }
        passwords.insert_for_commit(commit, password_file(Path::new(path))?);
    }
    Ok(passwords)
}

pub fn run(options: Options) -> Result<()> {
    let archive = fs::canonicalize(&options.archive)?;
    let mountpoint = fs::canonicalize(&options.mountpoint)?;
    if !mountpoint.is_dir() {
        return Err("mountpoint must be a directory".into());
    }
    if archive.starts_with(&mountpoint) {
        return Err("archive must not be inside the mountpoint".into());
    }
    let archive_name = archive.to_str().ok_or("archive path must be UTF-8")?;
    let passwords = passwords(&options)?;
    let prepared =
        PreparedExtraction::new_complete(archive_name, options.to.as_deref(), &passwords)?;
    let filesystem = ArchiveFs::new(prepared)?;
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::RO,
        MountOption::NoDev,
        MountOption::NoSuid,
        MountOption::NoExec,
        MountOption::DefaultPermissions,
        MountOption::FSName("zarmount".into()),
        MountOption::Subtype("zarmount".into()),
    ];
    fuser::mount(filesystem, mountpoint, &config)?;
    Ok(())
}

#[derive(Debug)]
struct Node {
    attr: FileAttr,
    parent: u64,
    path: String,
    target: Option<String>,
    children: BTreeMap<String, u64>,
}

struct ArchiveFs {
    archive: PreparedExtraction,
    nodes: Vec<Node>,
}

impl ArchiveFs {
    fn new(archive: PreparedExtraction) -> Result<Self> {
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        let mut nodes = vec![Node::new(
            ROOT,
            ROOT,
            String::new(),
            FileType::Directory,
            0,
            uid,
            gid,
        )];
        let mut paths = BTreeMap::from([(String::new(), ROOT)]);
        for (path, entry) in archive.toc().iter() {
            let (kind, size, target) = match entry {
                TocEntry::File(file) => (FileType::RegularFile, file.size, None),
                TocEntry::Directory => (FileType::Directory, 0, None),
                TocEntry::Symlink { target } if !target.contains('\0') => {
                    (FileType::Symlink, target.len() as u64, Some(target.clone()))
                }
                TocEntry::Symlink { .. } => {
                    return Err(format!("invalid link target for {path}").into());
                }
                TocEntry::NamedPipe | TocEntry::Device { .. } => {
                    return Err(format!("unsupported special file in archive: {path}").into());
                }
            };
            let components: Vec<_> = path.as_str().split('/').collect();
            let mut parent = ROOT;
            let mut name = String::new();
            for (index, component) in components.iter().enumerate() {
                if !name.is_empty() {
                    name.push('/');
                }
                name.push_str(component);
                let last = index + 1 == components.len();
                if let Some(&ino) = paths.get(&name) {
                    if !last && nodes[(ino - 1) as usize].attr.kind != FileType::Directory {
                        return Err(format!("not a directory: {name}").into());
                    }
                    if last && kind != FileType::Directory {
                        return Err(format!("conflicting archive entry: {name}").into());
                    }
                    parent = ino;
                    continue;
                }
                let ino = u64::try_from(nodes.len())?
                    .checked_add(1)
                    .ok_or("too many entries")?;
                let node_kind = if last { kind } else { FileType::Directory };
                let mut node = Node::new(
                    ino,
                    parent,
                    name.clone(),
                    node_kind,
                    if last { size } else { 0 },
                    uid,
                    gid,
                );
                if last {
                    node.target = target.clone();
                }
                nodes[(parent - 1) as usize]
                    .children
                    .insert((*component).to_owned(), ino);
                nodes.push(node);
                paths.insert(name.clone(), ino);
                parent = ino;
            }
        }
        for index in 0..nodes.len() {
            if nodes[index].attr.kind == FileType::Directory {
                let subdirectories = nodes[index]
                    .children
                    .values()
                    .filter(|&&ino| nodes[(ino - 1) as usize].attr.kind == FileType::Directory)
                    .count();
                nodes[index].attr.nlink = u32::try_from(subdirectories)?
                    .checked_add(2)
                    .ok_or("too many subdirectories")?;
            }
        }
        Ok(Self { archive, nodes })
    }

    fn node(&self, ino: INodeNo) -> Option<&Node> {
        usize::try_from(ino.0.checked_sub(1)?)
            .ok()
            .and_then(|index| self.nodes.get(index))
    }

    fn child(&self, parent: INodeNo, name: &OsStr) -> Option<&Node> {
        let parent = self.node(parent)?;
        let name = name.to_str()?;
        self.node(INodeNo(*parent.children.get(name)?))
    }
}

impl Node {
    fn new(
        ino: u64,
        parent: u64,
        path: String,
        kind: FileType,
        size: u64,
        uid: u32,
        gid: u32,
    ) -> Self {
        let perm = match kind {
            FileType::Directory => 0o555,
            FileType::Symlink => 0o777,
            _ => 0o444,
        };
        Self {
            attr: FileAttr {
                ino: INodeNo(ino),
                size,
                blocks: size.div_ceil(512),
                atime: UNIX_EPOCH,
                mtime: UNIX_EPOCH,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind,
                perm,
                nlink: if kind == FileType::Directory { 2 } else { 1 },
                uid,
                gid,
                rdev: 0,
                blksize: 4096,
                flags: 0,
            },
            parent,
            path,
            target: None,
            children: BTreeMap::new(),
        }
    }
}

impl Filesystem for ArchiveFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match self.child(parent, name) {
            Some(node) => reply.entry(&TTL, &node.attr, Generation(0)),
            None => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        match self.node(ino) {
            Some(node) => reply.attr(&TTL, &node.attr),
            None => reply.error(Errno::ENOENT),
        }
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        match self.node(ino) {
            Some(node) => match &node.target {
                Some(target) => reply.data(target.as_bytes()),
                None => reply.error(Errno::EINVAL),
            },
            None => reply.error(Errno::ENOENT),
        }
    }

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, _kill: bool, reply: ReplyOpen) {
        match self.node(ino) {
            None => reply.error(Errno::ENOENT),
            Some(node) if node.attr.kind == FileType::Directory => reply.error(Errno::EISDIR),
            Some(node) if node.attr.kind != FileType::RegularFile => reply.error(Errno::EINVAL),
            Some(_)
                if flags.0 & libc::O_ACCMODE != libc::O_RDONLY || flags.0 & libc::O_TRUNC != 0 =>
            {
                reply.error(Errno::EROFS)
            }
            Some(_) => reply.opened(FileHandle(0), FopenFlags::FOPEN_DIRECT_IO),
        }
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        match self.node(ino) {
            None => reply.error(Errno::ENOENT),
            Some(node) if node.attr.kind != FileType::RegularFile => reply.error(Errno::EINVAL),
            Some(node) => match self
                .archive
                .read_file_range(&node.path, offset, u64::from(size))
            {
                Ok(contents) => reply.data(&contents),
                Err(error) => {
                    eprintln!("zarmount: cannot read {}: {error}", node.path);
                    reply.error(Errno::EIO);
                }
            },
        }
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let Some(node) = self.node(ino) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if node.attr.kind != FileType::Directory {
            reply.error(Errno::ENOTDIR);
            return;
        }
        let entries = std::iter::once((".", ino.0, FileType::Directory))
            .chain(std::iter::once(("..", node.parent, FileType::Directory)))
            .chain(node.children.iter().map(|(name, child)| {
                (
                    name.as_str(),
                    *child,
                    self.nodes[(*child - 1) as usize].attr.kind,
                )
            }));
        for (index, (name, child, kind)) in entries.enumerate() {
            let next = index as u64 + 1;
            if next <= offset {
                continue;
            }
            if reply.add(INodeNo(child), next, kind, name) {
                break;
            }
        }
        reply.ok();
    }
}
