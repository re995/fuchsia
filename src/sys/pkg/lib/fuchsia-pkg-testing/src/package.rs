// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

//! Test tools for building Fuchsia packages.

use {
    crate::process::wait_for_process_termination,
    anyhow::{format_err, Context as _, Error},
    fdio::SpawnBuilder,
    fidl::endpoints::ServerEnd,
    fidl_fuchsia_io::{DirectoryProxy, FileMarker, FileObject, NodeInfo},
    fuchsia_merkle::{Hash, MerkleTree},
    fuchsia_pkg::{MetaContents, MetaPackage},
    fuchsia_zircon::{self as zx, prelude::*, Status},
    futures::{join, prelude::*},
    maplit::btreeset,
    std::{
        collections::{BTreeMap, BTreeSet, HashSet},
        fs::{self, File},
        io::{self, Read, Write},
        path::{Path, PathBuf},
    },
    tempfile::TempDir,
    walkdir::WalkDir,
};

/// A package generated by a [`PackageBuilder`], suitable for assembling into a TUF repository.
#[derive(Debug)]
pub struct Package {
    name: String,
    meta_far_merkle: Hash,
    artifacts: TempDir,
}

#[derive(Debug, PartialEq)]
enum PackageEntry {
    Directory,
    File(Vec<u8>),
}

impl PackageEntry {
    fn is_dir(&self) -> bool {
        match self {
            PackageEntry::Directory => true,
            _ => false,
        }
    }
}
pub struct BlobFile {
    pub merkle: fuchsia_merkle::Hash,
    pub file: File,
}

/// Contents of a Blob.
pub struct BlobContents {
    /// Merkle hash of the blob.
    pub merkle: fuchsia_merkle::Hash,

    /// Binary contents of the blob.
    pub contents: Vec<u8>,
}

impl Package {
    /// The merkle root of the package's meta.far.
    pub fn meta_far_merkle_root(&self) -> &Hash {
        &self.meta_far_merkle
    }

    /// The package's meta.far.
    pub fn meta_far(&self) -> io::Result<File> {
        File::open(self.artifacts.path().join("meta.far"))
    }

    /// The name of the package.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// The directory containing the blobs contained in the package, including the meta.far.
    pub fn artifacts(&self) -> &Path {
        self.artifacts.path()
    }

    /// Builds and returns the package located at "/pkg" in the current namespace.
    pub async fn identity() -> Result<Self, Error> {
        Self::from_dir("/pkg").await
    }

    /// Builds and returns the package located at the given path in the current namespace.
    pub async fn from_dir(root: impl AsRef<Path>) -> Result<Self, Error> {
        let root = root.as_ref();
        let file = File::open(root.join("meta/package"))?;
        let meta_package = MetaPackage::deserialize(io::BufReader::new(file))?;
        let mut pkg = PackageBuilder::new(meta_package.name());

        fn is_generated_file(path: &Path) -> bool {
            match path.to_str() {
                Some("meta/contents") => true,
                Some("meta/package") => true,
                _ => false,
            }
        }

        // Add all non-generated files from this package into `pkg`.
        for entry in WalkDir::new(root) {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type().is_file() || is_generated_file(path) {
                continue;
            }

            let relative_path = path.strip_prefix(root).unwrap();
            let f = File::open(path)?;
            pkg = pkg.add_resource_at(relative_path.to_str().unwrap(), f);
        }

        Ok(pkg.build().await?)
    }

    /// Returns the parsed contents of the meta/contents file.
    pub fn meta_contents(&self) -> Result<MetaContents, Error> {
        let mut raw_meta_far = self.meta_far()?;
        let mut meta_far = fuchsia_archive::Reader::new(&mut raw_meta_far)?;
        let raw_meta_contents = meta_far.read_file("meta/contents")?;

        Ok(MetaContents::deserialize(raw_meta_contents.as_slice())?)
    }

    /// Returns a set of all unique blobs contained in this package.
    pub fn list_blobs(&self) -> Result<BTreeSet<Hash>, Error> {
        let meta_contents = self.meta_contents()?;

        let mut res = btreeset![self.meta_far_merkle];
        res.extend(meta_contents.into_hashes());
        Ok(res)
    }

    /// Returns an iterator of merkle/File pairs for each content blob in the package.
    ///
    /// Does not include the meta.far, see `meta_far()` and `meta_far_merkle_root()`, instead.
    pub fn content_blob_files(&self) -> impl Iterator<Item = BlobFile> {
        let bytes = fs::read(self.artifacts().join("manifest.json")).expect("read manifest blob");
        let manifest: fuchsia_pkg::PackageManifest = serde_json::from_slice(&bytes).unwrap();
        struct Blob {
            merkle: fuchsia_merkle::Hash,
            path: PathBuf,
        }
        let blobs = manifest
            .into_blobs()
            .into_iter()
            .filter(|blob| blob.path != "meta/")
            .map(|blob| Blob {
                merkle: blob.merkle,
                path: self.artifacts().join(
                    // fixup source_path to be relative to artifacts by stripping
                    // "/packages/{name}/" off the front.
                    &blob.source_path[("/packages/".len() + self.name().len() + "/".len())..],
                ),
            })
            .collect::<Vec<_>>();

        blobs
            .into_iter()
            .map(|blob| BlobFile { merkle: blob.merkle, file: File::open(blob.path).unwrap() })
    }

    /// Returns a tuple of the contents of the meta far and the contents of all content blobs in the package.
    pub fn contents(&self) -> (BlobContents, Vec<BlobContents>) {
        (
            BlobContents {
                merkle: self.meta_far_merkle,
                contents: self.meta_far().unwrap().bytes().collect::<Result<Vec<u8>, _>>().unwrap(),
            },
            self.content_blob_files()
                .map(|blob_file| BlobContents {
                    merkle: blob_file.merkle,
                    contents: blob_file.file.bytes().collect::<Result<Vec<u8>, _>>().unwrap(),
                })
                .collect(),
        )
    }

    /// Puts all the blobs for the package in blobfs.
    pub fn write_to_blobfs_dir(&self, dir: &openat::Dir) {
        use std::convert::TryInto;

        fn write_blob(
            dir: &openat::Dir,
            merkle: &fuchsia_merkle::Hash,
            mut source: impl std::io::Read,
        ) -> Result<(), anyhow::Error> {
            let mut bytes = vec![];
            source.read_to_end(&mut bytes)?;
            let mut file = match dir.write_file(merkle.to_string(), 0o777) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    // blobfs already aware of this blob (e.g. blob is written or write is in-flight)
                    return Ok(());
                }
                Err(e) => Err(e)?,
            };
            file.set_len(bytes.len().try_into().unwrap())?;
            file.write_all(&bytes)?;
            Ok(())
        }

        write_blob(dir, &self.meta_far_merkle_root(), self.meta_far().unwrap()).unwrap();
        for blob in self.content_blob_files() {
            write_blob(dir, &blob.merkle, blob.file).unwrap();
        }
    }

    /// Verifies that the given directory serves the contents of this package.
    pub async fn verify_contents(&self, dir: &DirectoryProxy) -> Result<(), VerificationError> {
        let mut raw_meta_far = self.meta_far()?;
        let mut meta_far = fuchsia_archive::Reader::new(&mut raw_meta_far)?;
        let mut expected_paths = HashSet::new();

        // Verify all entries referenced by meta/contents exist and have the correct merkle root.
        let raw_meta_contents = meta_far.read_file("meta/contents")?;
        let meta_contents = MetaContents::deserialize(raw_meta_contents.as_slice())?;
        for (path, merkle) in meta_contents.contents() {
            let actual_merkle =
                MerkleTree::from_reader(read_file(dir, path).await?.as_slice())?.root();
            if merkle != &actual_merkle {
                return Err(VerificationError::DifferentFileData { path: path.to_owned() });
            }
            expected_paths.insert(path.clone());
        }

        // Verify all entries in the meta FAR exist and have the correct contents.
        for path in meta_far.list().map(|e| e.path().to_string()).collect::<Vec<_>>() {
            if read_file(dir, path.as_str()).await? != meta_far.read_file(path.as_str())? {
                return Err(VerificationError::DifferentFileData { path });
            }
            expected_paths.insert(path);
        }

        // Verify no other entries exist in the served directory.
        let mut stream = files_async::readdir_recursive(&dir, /*timeout=*/ None);
        while let Some(entry) = stream.try_next().await? {
            let path = entry.name;
            if !expected_paths.contains(path.as_str()) {
                return Err(VerificationError::ExtraFile { path });
            }
        }

        Ok(())
    }
}

async fn read_file(dir: &DirectoryProxy, path: &str) -> Result<Vec<u8>, VerificationError> {
    let (file, server_end) = fidl::endpoints::create_proxy::<FileMarker>().unwrap();

    let flags = fidl_fuchsia_io::OPEN_FLAG_DESCRIBE | fidl_fuchsia_io::OPEN_RIGHT_READABLE;
    dir.open(flags, 0, path, ServerEnd::new(server_end.into_channel()))
        .expect("open request to send");

    let mut events = file.take_event_stream();
    let open = async move {
        let fidl_fuchsia_io::FileEvent::OnOpen_ { s, info } =
            events.next().await.expect("Some(event)").expect("no fidl error");

        match Status::ok(s) {
            Err(Status::NOT_FOUND) => Err(VerificationError::MissingFile { path: path.to_owned() }),
            Err(status) => Err(format_err!("unable to open {:?}: {:?}", path, status).into()),
            Ok(()) => Ok(()),
        }?;

        let event = match *info.expect("FileEvent to have NodeInfo") {
            NodeInfo::File(FileObject { event, stream: None }) => event,
            other => {
                panic!("NodeInfo from FileEventStream to be File variant with event: {:?}", other)
            }
        };

        // Files served by the package will either provide an event in its describe info (if that
        // file is actually a blob from blobfs) or not provide an event (if that file is, for
        // example, a file contained within the meta far being served in the meta/ directory).
        //
        // If the file is a blobfs blob, we want to make sure it is readable. We can just try to
        // read from it, but the preferred method to wait for a blobfs blob to become readable is
        // to wait on the USER_0 signal to become asserted on the file's event.
        //
        // As all blobs served by a package should already be readable, we assert that USER_0 is
        // already asserted on the event.
        if let Some(event) = event {
            match event.wait_handle(zx::Signals::USER_0, zx::Time::after(0.seconds())) {
                Err(Status::TIMED_OUT) => Err(VerificationError::from(format_err!(
                    "file served by blobfs is not complete/readable as USER_0 signal was not set on the File's event: {}",
                    path
                ))),
                Err(other_status) => Err(VerificationError::from(format_err!(
                    "wait_handle failed with status: {:?} {:?}",
                    other_status,
                    path
                ))),
                Ok(_) => Ok(()),
            }
        } else {
            Ok(())
        }
    };

    let read = async {
        let (status, buffer) = file.get_buffer(fidl_fuchsia_io::VMO_FLAG_READ).await?;
        let mut expect_empty_blob = false;

        // First try getting a fuchsia.mem.Buffer of the file, as blobfs already has a VMO for the
        // blob and it is faster to get the file data that way. Not all package blobs support the
        // API, so on expected errors, fall back to reading the file over FIDL.
        match Status::from_raw(status) {
            Status::OK => {
                let buffer = buffer.unwrap();
                let mut buf = vec![0u8; buffer.size as usize];
                buffer
                    .vmo
                    .read(&mut buf[..], 0)
                    .map_err(|e| format_err!("unable to read from vmo: {}", e))?;
                return Ok(buf);
            }
            Status::NOT_SUPPORTED => {
                // meta far files do not support get_buffer, fallback to file read path.
            }
            Status::BAD_STATE => {
                // may or may not be intended behavior, but the empty blob will not provide a vmo,
                // failing with BAD_STATE. Verify in the read path below that the blob is indeed
                // zero length if this happens.
                expect_empty_blob = true;
            }
            status => {
                return Err(VerificationError::from(format_err!(
                    "unexpected error opening file buffer: {:?}",
                    status
                )));
            }
        }

        let mut buf = vec![];
        loop {
            let (status, chunk) =
                file.read(fidl_fuchsia_io::MAX_BUF).await.context("file read to respond")?;
            Status::ok(status)
                .map_err(|status| VerificationError::FileReadError { path: path.into(), status })?;

            if chunk.is_empty() {
                if expect_empty_blob {
                    assert_eq!(buf, Vec::<u8>::new());
                }
                return Ok(buf);
            }

            buf.extend(chunk);
        }
    };

    let (open, read) = join!(open, read);
    let close_result = file.close().await.context("file close to respond");
    let result = open.and(read)?;
    // Only check close_result if everything that came before it looks good.
    Status::ok(close_result?)
        .map_err(|status| format_err!("unable to close {:?}: {:?}", path, status))?;
    Ok(result)
}

/// An error that can occur while verifying the contents of a directory.
#[derive(Debug)]
pub enum VerificationError {
    /// The directory is serving a file that isn't in the package.
    ExtraFile {
        /// Path to the extra file.
        path: String,
    },
    /// The directory is not serving a particular file that it should be serving.
    MissingFile {
        /// Path to the missing file.
        path: String,
    },
    /// The actual merkle of the file does not match the merkle listed in the meta FAR.
    DifferentFileData {
        /// Path to the file.
        path: String,
    },
    /// Read method on file failed.
    FileReadError {
        /// Path to the file
        path: String,
        /// Read result
        status: Status,
    },
    /// Anything else.
    Other(Error),
}

impl<T: Into<Error>> From<T> for VerificationError {
    fn from(x: T) -> Self {
        VerificationError::Other(x.into())
    }
}

/// A builder to simplify construction of Fuchsia packages.
pub struct PackageBuilder {
    name: String,
    contents: BTreeMap<PathBuf, PackageEntry>,
}

impl PackageBuilder {
    /// Creates a new `PackageBuilder`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), contents: BTreeMap::new() }
    }

    /// Create a subdirectory within the package.
    ///
    /// # Panics
    ///
    /// Panics if the package contains a file entry at `path` or any of its ancestors.
    pub fn dir(mut self, path: impl Into<PathBuf>) -> PackageDir {
        let path = path.into();
        self.make_dirs(&path);
        PackageDir::new(self, path)
    }

    /// Adds the provided `contents` to the package at the given `path`.
    ///
    /// # Panics
    ///
    /// Panics if either:
    /// * The package already contains a file or directory at `path`.
    /// * The package contains a file at any of `path`'s ancestors.
    pub fn add_resource_at(
        mut self,
        path: impl Into<PathBuf>,
        mut contents: impl io::Read,
    ) -> Self {
        let path = path.into();
        let mut data = vec![];
        contents.read_to_end(&mut data).unwrap();

        if let Some(parent) = path.parent() {
            self.make_dirs(parent);
        }
        let replaced = self.contents.insert(path.clone(), PackageEntry::File(data));
        assert_eq!(None, replaced, "already contains an entry at {:?}", path);
        self
    }

    fn make_dirs(&mut self, path: &Path) {
        for ancestor in path.ancestors() {
            if ancestor == Path::new("") {
                continue;
            }
            assert!(
                self.contents
                    .entry(ancestor.to_owned())
                    .or_insert(PackageEntry::Directory)
                    .is_dir(),
                "{:?} is not a directory",
                ancestor
            );
        }
    }

    /// Builds the package.
    pub async fn build(self) -> Result<Package, Error> {
        // TODO Consider switching indir/packagedir to be a VFS instead

        // indir contains temporary inputs to package creation
        let indir = tempfile::tempdir().context("create /in")?;

        // packagedir contains outputs from package creation (manifest.json/meta.far) as well as
        // all blobs contained in the package. Pm will build the package manifest with absolute
        // source paths to /packages/{self.name}/contents/*.
        //
        // Sample layout of packagedir:
        // - {name}/
        // -   manifest.json
        // -   meta.far
        // -   contents/
        // -     file{N}

        let packagedir = tempfile::tempdir().context("create /packages")?;
        fs::create_dir(packagedir.path().join("contents")).context("create /packages/contents")?;
        let package_mount_path = format!("/packages/{}", &self.name);

        {
            let mut manifest = File::create(indir.path().join("package.manifest"))?;

            MetaPackage::from_name_and_variant(self.name.clone(), "0")?
                .serialize(File::create(indir.path().join("meta_package"))?)?;
            writeln!(manifest, "meta/package=/in/meta_package")?;

            for (i, (name, contents)) in self.contents.iter().enumerate() {
                let contents = match contents {
                    PackageEntry::File(data) => data,
                    _ => continue,
                };
                let path = format!("file{}", i);
                fs::write(packagedir.path().join("contents").join(&path), contents)?;
                writeln!(
                    manifest,
                    "{}={}/contents/{}",
                    &name.to_string_lossy(),
                    package_mount_path,
                    path
                )?;
            }
        }

        let pm = SpawnBuilder::new()
            .options(fdio::SpawnOptions::CLONE_ALL - fdio::SpawnOptions::CLONE_NAMESPACE)
            .arg("pm")?
            .arg(format!("-n={}", self.name))?
            .arg("-version=0")?
            .arg("-m=/in/package.manifest")?
            .arg(format!("-o={}", package_mount_path))?
            .arg("build")?
            .arg("-depfile=false")?
            .arg(format!("-output-package-manifest={}/manifest.json", &package_mount_path))?
            .add_dir_to_namespace("/in".to_owned(), File::open(indir.path()).context("open /in")?)?
            .add_dir_to_namespace(
                package_mount_path.clone(),
                File::open(packagedir.path()).context("open /packages")?,
            )?
            .spawn_from_path("/pkg/bin/pm", &fuchsia_runtime::job_default())
            .context("spawning pm to build package")?;

        wait_for_process_termination(pm).await.context("waiting for pm to build package")?;

        let meta_far_merkle =
            fs::read_to_string(packagedir.path().join("meta.far.merkle"))?.parse()?;

        // clean up after pm
        fs::remove_file(packagedir.path().join("meta/contents"))?;
        fs::remove_dir(packagedir.path().join("meta"))?;
        fs::remove_file(packagedir.path().join("meta.far.merkle"))?;

        Ok(Package { name: self.name, meta_far_merkle, artifacts: packagedir })
    }
}

/// A subdirectory of a package being built.
pub struct PackageDir {
    pkg: PackageBuilder,
    path: PathBuf,
}

impl PackageDir {
    fn new(pkg: PackageBuilder, path: impl Into<PathBuf>) -> Self {
        Self { pkg, path: path.into() }
    }

    /// Adds the provided `contents` to the package at the given `path`, relative to this
    /// `PackageDir`.
    ///
    /// # Panics
    /// If the package already contains a resource at `path`, relative to this `PackageDir`.
    pub fn add_resource_at(mut self, path: impl AsRef<Path>, contents: impl io::Read) -> Self {
        self.pkg = self.pkg.add_resource_at(self.path.join(path.as_ref()), contents);
        self
    }

    /// Finish adding resources to this directory, returning the modified [`PackageBuilder`].
    pub fn finish(self) -> PackageBuilder {
        self.pkg
    }
}

#[cfg(test)]
mod tests {
    use {super::*, fuchsia_merkle::MerkleTree, matches::assert_matches, std::io::Read};

    #[test]
    #[should_panic(expected = r#""data" is not a directory"#)]
    fn test_panics_file_with_existing_parent_as_file() {
        let _ = (|| -> Result<(), Error> {
            PackageBuilder::new("test")
                .add_resource_at("data", "data contents".as_bytes())
                .add_resource_at("data/foo", "data/foo contents".as_bytes());
            Ok(())
        })();
    }

    #[test]
    #[should_panic(expected = r#""data" is not a directory"#)]
    fn test_panics_dir_with_existing_file() {
        let _ = (|| -> Result<(), Error> {
            PackageBuilder::new("test")
                .add_resource_at("data", "data contents".as_bytes())
                .dir("data");
            Ok(())
        })();
    }

    #[test]
    #[should_panic(expected = r#""data" is not a directory"#)]
    fn test_panics_nested_dir_with_existing_file() {
        let _ = (|| -> Result<(), Error> {
            PackageBuilder::new("test")
                .add_resource_at("data", "data contents".as_bytes())
                .dir("data/foo");
            Ok(())
        })();
    }

    #[test]
    #[should_panic(expected = r#"already contains an entry at "data""#)]
    fn test_panics_file_with_existing_dir() {
        let _ = (|| -> Result<(), Error> {
            PackageBuilder::new("test")
                .dir("data")
                .add_resource_at("foo", "data/foo contents".as_bytes())
                .finish()
                .add_resource_at("data", "data contents".as_bytes());
            Ok(())
        })();
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_basic() -> Result<(), Error> {
        let pkg = PackageBuilder::new("rolldice")
            .dir("bin")
            .add_resource_at("rolldice", "asldkfjaslkdfjalskdjfalskdf".as_bytes())
            .finish()
            .build()
            .await?;

        assert_eq!(
            pkg.meta_far_merkle,
            "7b3591496961a5b8918525feb2e49e6d1439580fd805a2b4178fc7581c011a0d".parse()?
        );
        assert_eq!(pkg.meta_far_merkle, MerkleTree::from_reader(pkg.meta_far()?)?.root());
        assert_eq!(
            pkg.list_blobs()?,
            btreeset![
                "7b3591496961a5b8918525feb2e49e6d1439580fd805a2b4178fc7581c011a0d".parse()?,
                "b5b34f6234631edc7ccaa25533e2050e5d597a7331c8974306b617a3682a3197".parse()?
            ]
        );

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_content_blob_files() -> Result<(), Error> {
        let pkg = PackageBuilder::new("rolldice")
            .dir("bin")
            .add_resource_at("rolldice", "asldkfjaslkdfjalskdjfalskdf".as_bytes())
            .add_resource_at("rolldice2", "asldkfjaslkdfjalskdjfalskdf".as_bytes())
            .finish()
            .build()
            .await?;

        let mut iter = pkg.content_blob_files();
        // 2 identical entries
        for _ in 0..2 {
            let BlobFile { merkle, mut file } = iter.next().unwrap();
            assert_eq!(
                merkle,
                "b5b34f6234631edc7ccaa25533e2050e5d597a7331c8974306b617a3682a3197".parse().unwrap()
            );
            let mut contents = vec![];
            file.read_to_end(&mut contents).unwrap();
            assert_eq!(contents, b"asldkfjaslkdfjalskdjfalskdf")
        }
        assert_eq!(iter.next().map(|b| b.merkle), None);

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_dir_semantics() -> Result<(), Error> {
        let with_dir = PackageBuilder::new("data-file")
            .dir("data")
            .add_resource_at("file", "contents".as_bytes())
            .finish()
            .build()
            .await?;

        let with_direct = PackageBuilder::new("data-file")
            .add_resource_at("data/file", "contents".as_bytes())
            .build()
            .await?;

        assert_eq!(with_dir.meta_far_merkle_root(), with_direct.meta_far_merkle_root());

        Ok(())
    }

    /// Creates a clone of the contents of /pkg in a tempdir so that tests can manipulate its
    /// contents.
    fn make_this_package_dir() -> Result<tempfile::TempDir, Error> {
        let dir = tempfile::tempdir()?;

        let this_package_root = Path::new("/pkg");

        for entry in WalkDir::new(this_package_root) {
            let entry = entry?;
            let path = entry.path();

            let relative_path = path.strip_prefix(this_package_root).unwrap();
            let rebased_path = dir.path().join(relative_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(rebased_path)?;
            } else if entry.file_type().is_file() {
                fs::copy(path, rebased_path)?;
            }
        }

        Ok(dir)
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_from_dir() -> Result<(), Error> {
        let root = {
            let dir = tempfile::tempdir()?;
            fs::create_dir(dir.path().join("meta"))?;
            fs::create_dir(dir.path().join("data"))?;
            MetaPackage::from_name_and_variant("asdf", "0")?
                .serialize(File::create(dir.path().join("meta/package"))?)?;
            fs::write(dir.path().join("data/hello"), "world")?;
            dir
        };

        let from_dir = Package::from_dir(root.path()).await?;
        let pkg = PackageBuilder::new("asdf")
            .add_resource_at("data/hello", "world".as_bytes())
            .build()
            .await?;

        assert_eq!(from_dir.meta_far_merkle, pkg.meta_far_merkle);

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_identity() -> Result<(), Error> {
        let pkg = Package::identity().await?;

        assert_eq!(pkg.meta_far_merkle, MerkleTree::from_reader(pkg.meta_far()?)?.root());

        // Verify the generated package's merkle root is the same as this test package's merkle root.
        assert_eq!(pkg.meta_far_merkle, fs::read_to_string("/pkg/meta")?.parse()?);

        let this_pkg_dir =
            io_util::open_directory_in_namespace("/pkg", io_util::OPEN_RIGHT_READABLE)?;
        pkg.verify_contents(&this_pkg_dir).await.expect("contents to be equivalent");

        let pkg_dir = make_this_package_dir()?;

        let this_pkg_dir = io_util::open_directory_in_namespace(
            pkg_dir.path().to_str().unwrap(),
            io_util::OPEN_RIGHT_READABLE,
        )?;

        assert_matches!(pkg.verify_contents(&this_pkg_dir).await, Ok(()));

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_verify_contents_rejects_extra_blob() -> Result<(), Error> {
        let pkg = Package::identity().await?;
        let pkg_dir = make_this_package_dir()?;

        fs::write(pkg_dir.path().join("unexpected"), "unexpected file".as_bytes())?;

        let pkg_dir_proxy = io_util::open_directory_in_namespace(
            pkg_dir.path().to_str().unwrap(),
            io_util::OPEN_RIGHT_READABLE,
        )?;

        assert_matches!(
            pkg.verify_contents(&pkg_dir_proxy).await,
            Err(VerificationError::ExtraFile{ref path}) if path == "unexpected");

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_verify_contents_rejects_extra_meta_file() -> Result<(), Error> {
        let pkg = Package::identity().await?;
        let pkg_dir = make_this_package_dir()?;

        fs::write(pkg_dir.path().join("meta/unexpected"), "unexpected file".as_bytes())?;

        let pkg_dir_proxy = io_util::open_directory_in_namespace(
            pkg_dir.path().to_str().unwrap(),
            io_util::OPEN_RIGHT_READABLE,
        )?;

        assert_matches!(
            pkg.verify_contents(&pkg_dir_proxy).await,
            Err(VerificationError::ExtraFile{ref path}) if path == "meta/unexpected");

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_verify_contents_rejects_missing_blob() -> Result<(), Error> {
        let pkg = Package::identity().await?;
        let pkg_dir = make_this_package_dir()?;

        fs::remove_file(pkg_dir.path().join("bin/pkgsvr"))?;

        let pkg_dir_proxy = io_util::open_directory_in_namespace(
            pkg_dir.path().to_str().unwrap(),
            io_util::OPEN_RIGHT_READABLE,
        )?;

        assert_matches!(
            pkg.verify_contents(&pkg_dir_proxy).await,
            Err(VerificationError::MissingFile{ref path}) if path == "bin/pkgsvr");

        Ok(())
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_verify_contents_rejects_different_contents() -> Result<(), Error> {
        let pkg = Package::identity().await?;
        let pkg_dir = make_this_package_dir()?;

        fs::write(pkg_dir.path().join("bin/pkgsvr"), "broken".as_bytes())?;

        let pkg_dir_proxy = io_util::open_directory_in_namespace(
            pkg_dir.path().to_str().unwrap(),
            io_util::OPEN_RIGHT_READABLE,
        )?;

        assert_matches!(
            pkg.verify_contents(&pkg_dir_proxy).await,
            Err(VerificationError::DifferentFileData{ref path}) if path == "bin/pkgsvr");

        Ok(())
    }
}
