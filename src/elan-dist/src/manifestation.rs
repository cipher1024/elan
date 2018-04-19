//! Manifest a particular Lean version by installing it from a distribution server.

use dist::TargetTriple;
use component::{Components, Transaction, TarGzPackage, ZipPackage, Package};
use temp;
use errors::*;
use notifications::*;
use download::DownloadCfg;
use prefix::InstallPrefix;

#[derive(Debug)]
pub struct Manifestation {
    installation: Components,
    target_triple: TargetTriple
}

impl Manifestation {
    /// Open the install prefix for updates from a distribution
    /// channel.  The install prefix directory does not need to exist;
    /// it will be created as needed. If there's an existing install
    /// then the lean-install installation format will be verified. A
    /// bad installer version is the only reason this will fail.
    pub fn open(prefix: InstallPrefix, triple: TargetTriple) -> Result<Self> {
        // TODO: validate the triple with the existing install as well
        // as the metadata format of the existing install
        Ok(Manifestation {
            installation: try!(Components::open(prefix)),
            target_triple: triple,
        })
    }

    /// Installation using the legacy v1 manifest format
    pub fn update(&self,
                  url: &String,
                  temp_cfg: &temp::Cfg,
                  notify_handler: &Fn(Notification)) -> Result<()> {
        notify_handler(Notification::DownloadingComponent("lean",
                                                          &self.target_triple,
                                                          Some(&self.target_triple)));

        use std::path::PathBuf;
        let dld_dir = PathBuf::from("bogus");
        let dlcfg = DownloadCfg {
            download_dir: &dld_dir,
            temp_cfg: temp_cfg,
            notify_handler: notify_handler
        };

        // find correct download on HTML page (AAAAH)
        use std::fs;
        use regex::Regex;
        use std::io::Read;
        let informal_target = match self.target_triple.0.as_str() {
            "x86_64-unknown-linux-gnu" => Some("linux"),
            "x86_64-apple-darwin" => Some("darwin"),
            "x86_64-pc-windows-msvc" => Some("windows"),
            _ => None,
        };
        let re = Regex::new(r#"/leanprover/[a-z-]+/releases/download/[^"]+"#).unwrap();
        let download_page_file = dlcfg.download_and_check(&url, "")?;
        let mut html = String::new();
        fs::File::open(&download_page_file as &::std::path::Path)?.read_to_string(&mut html)?;
        let url = re.find_iter(&html).map(|m| m.as_str().to_string()).find(|m|
            informal_target.map(|t| m.as_str().contains(t)).unwrap_or(false));
        if url.is_none() {
            return Err(format!("binary package was not provided for '{}'",
                               self.target_triple.to_string()).into());
        }
        let url = format!("https://github.com/{}", url.unwrap());

        let ext = if cfg!(target_os = "linux") { ".tar.gz" } else { ".zip" };
        let installer_file = try!(dlcfg.download_and_check(&url, ext));

        let prefix = self.installation.prefix();

        notify_handler(Notification::InstallingComponent("lean",
                                                         &self.target_triple,
                                                         Some(&self.target_triple)));

        // Begin transaction
        let mut tx = Transaction::new(prefix.clone(), temp_cfg, notify_handler);

        // Uninstall components
        for component in try!(self.installation.list()) {
            tx = try!(component.uninstall(tx));
        }

        // Install all the components in the installer
        let package: Box<Package> = if cfg!(target_os = "linux") {
            Box::new(try!(TarGzPackage::new_file(&installer_file, temp_cfg)))
        } else {
            Box::new(try!(ZipPackage::new_file(&installer_file, temp_cfg)))
        };

        for component in package.components() {
            tx = try!(package.install(&self.installation,
                                      &component, None,
                                      tx));
        }

        // End transaction
        tx.commit();

        Ok(())
    }
}
