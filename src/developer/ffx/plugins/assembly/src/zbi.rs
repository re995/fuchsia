// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::base_package::BasePackage;
use crate::config::{BoardConfig, ProductConfig, ZbiSigningScript};
use crate::util::pkg_manifest_from_path;

use anyhow::{anyhow, Context, Result};
use assembly_util::PathToStringExt;
use fuchsia_pkg::PackageManifest;
use std::path::{Path, PathBuf};
use std::process::Command;
use zbi::ZbiBuilder;

pub fn construct_zbi(
    outdir: impl AsRef<Path>,
    gendir: impl AsRef<Path>,
    product: &ProductConfig,
    board: &BoardConfig,
    base_package: Option<&BasePackage>,
) -> Result<PathBuf> {
    let mut zbi_builder = ZbiBuilder::default();

    // Add the kernel image.
    zbi_builder.set_kernel(&product.kernel_image);

    // Instruct devmgr that a /system volume is required.
    zbi_builder.add_boot_arg("devmgr.require-system=true");

    // If a base merkle is supplied, then add the boot arguments for starting up pkgfs with the
    // merkle of the Base Package.
    if let Some(base_package) = &base_package {
        // Specify how to launch pkgfs: bin/pkgsvr <base-merkle>
        zbi_builder
            .add_boot_arg(&format!("zircon.system.pkgfs.cmd=bin/pkgsvr+{}", &base_package.merkle));

        // Add the pkgfs blobs to the boot arguments, so that pkgfs can be bootstrapped out of blobfs,
        // before the blobfs service is available.
        let pkgfs_manifest: PackageManifest = product
            .base_packages
            .iter()
            .find_map(|p| {
                if let Ok(m) = pkg_manifest_from_path(p) {
                    if m.name() == "pkgfs" {
                        return Some(m);
                    }
                }
                return None;
            })
            .context("Failed to find pkgfs in the base packages")?;

        pkgfs_manifest.into_blobs().into_iter().filter(|b| b.path != "meta/").for_each(|b| {
            zbi_builder.add_boot_arg(&format!("zircon.system.pkgfs.file.{}={}", b.path, b.merkle));
        });
    }

    // Add the command line.
    for cmd in &product.kernel_cmdline {
        zbi_builder.add_cmdline_arg(cmd);
    }

    // Add the BootFS files.
    for bootfs_entry in &product.bootfs_files {
        zbi_builder.add_bootfs_file(&bootfs_entry.source, &bootfs_entry.destination);
    }

    // Set the zbi compression to use.
    zbi_builder.set_compression(&board.zbi.compression);

    // Create an output manifest that describes the contents of the built ZBI.
    zbi_builder.set_output_manifest(&gendir.as_ref().join("zbi.json"));

    // Build and return the ZBI.
    let zbi_path = if board.zbi.signing_script.is_none() {
        outdir.as_ref().join("fuchsia.zbi")
    } else {
        outdir.as_ref().join("fuchsia.zbi.unsigned")
    };
    zbi_builder.build(gendir, zbi_path.as_path())?;
    Ok(zbi_path)
}

/// If the board requires the zbi to be post-processed to make it bootable by
/// the bootloaders, then perform that task here.
pub fn vendor_sign_zbi(
    outdir: impl AsRef<Path>,
    signing_config: &ZbiSigningScript,
    zbi: impl AsRef<Path>,
) -> Result<PathBuf> {
    // The resultant file path
    let signed_path = outdir.as_ref().join("fuchsia.zbi");

    // The parameters of the script that are required:
    let mut args = Vec::new();
    args.push("-z".to_string());
    args.push(zbi.as_ref().path_to_string()?);
    args.push("-o".to_string());
    args.push(signed_path.path_to_string()?);

    // If the script config defines extra arguments, add them:
    args.extend_from_slice(&signing_config.extra_arguments[..]);

    let output = Command::new(&signing_config.tool)
        .args(&args)
        .output()
        .context(format!("Failed to run the vendor tool: {}", signing_config.tool.display()))?;

    // The tool ran, but may have returned an error.
    if output.status.success() {
        Ok(signed_path)
    } else {
        Err(anyhow!(
            "Vendor tool returned an error: {}\n{}",
            output.status,
            String::from_utf8_lossy(output.stderr.as_slice())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{construct_zbi, vendor_sign_zbi};

    use crate::base_package::BasePackage;
    use crate::config::{BlobFSConfig, BoardConfig, ProductConfig, ZbiConfig, ZbiSigningScript};
    use assembly_util::PathToStringExt;
    use fuchsia_hash::Hash;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn construct() {
        let dir = tempdir().unwrap();

        // Create fake product/board definitions.
        let mut product_config = ProductConfig::default();
        let board_config = BoardConfig {
            board_name: "board".to_string(),
            vbmeta: None,
            bootloaders: Vec::new(),
            zbi: ZbiConfig {
                partition: "zbi".to_string(),
                name: "fuchsia".to_string(),
                max_size: 0,
                embed_fvm_in_zbi: false,
                compression: "zstd".to_string(),
                signing_script: None,
            },
            blobfs: BlobFSConfig::default(),
            fvm: None,
            recovery: None,
        };

        // Create a kernel which is equivalent to: zbi --ouput <zbi-name>
        let kernel_path = dir.path().join("kernel");
        let kernel_bytes = vec![
            0x42, 0x4f, 0x4f, 0x54, 0x00, 0x00, 0x00, 0x00, 0xe6, 0xf7, 0x8c, 0x86, 0x00, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x17, 0x78, 0xb5,
            0xd6, 0xe8, 0x87, 0x4a,
        ];
        std::fs::write(&kernel_path, kernel_bytes).unwrap();
        product_config.kernel_image = kernel_path;

        // Create a fake pkgfs.
        let pkgfs_manifest_path = generate_test_manifest_file(dir.path(), "pkgfs");
        product_config.base_packages.push(pkgfs_manifest_path);

        // Create a fake base package.
        let base_path = dir.path().join("base.far");
        std::fs::write(&base_path, "fake base").unwrap();
        let base = BasePackage {
            merkle: Hash::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            contents: BTreeMap::default(),
            path: base_path,
        };

        let zbi_path =
            construct_zbi(dir.path(), dir.path(), &product_config, &board_config, Some(&base))
                .unwrap();
        assert_eq!(zbi_path, dir.path().join("fuchsia.zbi"));
    }

    #[test]
    fn vendor_sign() {
        let dir = tempdir().unwrap();
        let expected_output = dir.path().join("fuchsia.zbi");

        // Create a fake zbi.
        let zbi_path = dir.path().join("fuchsia.zbi.unsigned");
        std::fs::write(&zbi_path, "fake zbi").unwrap();

        // Create the signing tool that ensures that we pass the correct arguments.
        let tool_string = format!(
            r#"#!/bin/bash
            if [[ "$1" != "-z" || "$3" != "-o" ]]; then
              exit 1
            fi
            if [[ "$2" != {} ]]; then
              exit 1
            fi
            if [[ "$4" != {} ]]; then
              exit 1
            fi
            if [[ "$5" != {} || "$6" != {} ]]; then
              exit 1
            fi
            exit 0
        "#,
            zbi_path.path_to_string().unwrap(),
            expected_output.path_to_string().unwrap(),
            "arg1",
            "arg2",
        );
        let tool_path = dir.path().join("tool.sh");
        std::fs::write(&tool_path, tool_string.as_bytes()).unwrap();
        std::fs::set_permissions(&tool_path, std::fs::Permissions::from_mode(0o500)).unwrap();

        // Create the signing config.
        let extra_arguments = vec!["arg1".into(), "arg2".into()];
        let signing_config = ZbiSigningScript { tool: tool_path, extra_arguments };

        // Sign the zbi.
        let signed_zbi_path = vendor_sign_zbi(dir.path(), &signing_config, &zbi_path).unwrap();
        assert_eq!(signed_zbi_path, expected_output);
    }

    // Generates a package manifest to be used for testing. The file is written
    // into `dir`, and the location is returned. The `name` is used in the blob
    // file names to make each manifest somewhat unique.
    // TODO(fxbug.dev/76993): See if we can share this with BasePackage.
    pub fn generate_test_manifest_file(dir: impl AsRef<Path>, name: impl AsRef<str>) -> PathBuf {
        // Create a data file for the package.
        let data_file_name = format!("{}_data.txt", name.as_ref());
        let data_path = dir.as_ref().join(&data_file_name);
        let data_file = File::create(&data_path).unwrap();
        write!(&data_file, "bleh").unwrap();

        // Create the manifest.
        let manifest_path = dir.as_ref().join(format!("{}.json", name.as_ref()));
        let manifest_file = File::create(&manifest_path).unwrap();
        serde_json::to_writer(
            &manifest_file,
            &json!({
                    "version": "1",
                    "package": {
                        "name": name.as_ref(),
                        "version": "1",
                    },
                    "blobs": [
                        {
                            "source_path": format!("path/to/{}/meta.far", name.as_ref()),
                            "path": "meta/",
                            "merkle":
                                "0000000000000000000000000000000000000000000000000000000000000000",
                            "size": 1
                        },
                        {
                            "source_path": &data_path,
                            "path": &data_file_name,
                            "merkle":
                                "1111111111111111111111111111111111111111111111111111111111111111",
                            "size": 1
                        },
                    ]
                }
            ),
        )
        .unwrap();
        manifest_path
    }
}
