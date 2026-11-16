//! `.netlix` files: JSON with a format tag and version, so future versions can migrate.

use crate::model::Topology;
use crate::tf;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXTENSION: &str = "netlix";
const FORMAT: &str = "netlix";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct FileV1 {
    format: String,
    version: u32,
    topology: Topology,
}

pub fn to_string(topo: &Topology) -> String {
    let file = FileV1 {
        format: FORMAT.into(),
        version: VERSION,
        topology: topo.clone(),
    };
    serde_json::to_string_pretty(&file).expect("topology is always serialisable")
}

pub fn from_str(text: &str) -> Result<Topology, String> {
    let file: FileV1 = serde_json::from_str(text).map_err(|e| {
        tf!(
            "Ce n'est pas un fichier netlix valide : {e}",
            "Not a valid netlix file: {e}"
        )
    })?;
    if file.format != FORMAT {
        let format = &file.format;
        return Err(tf!(
            "Format de fichier inconnu « {format} ».",
            "Unknown file format \"{format}\"."
        ));
    }
    if file.version > VERSION {
        let v = file.version;
        return Err(tf!(
            "Ce fichier vient d'une version plus récente de netlix (format v{v}). Mettez netlix à jour.",
            "This file was made by a newer netlix (format v{v}). Please update."
        ));
    }
    Ok(file.topology)
}

pub fn save(path: &Path, topo: &Topology) -> Result<(), String> {
    let p = path.display();
    std::fs::write(path, to_string(topo)).map_err(|e| {
        tf!(
            "Impossible d'enregistrer {p} : {e}",
            "Couldn't save {p}: {e}"
        )
    })
}

pub fn load(path: &Path) -> Result<Topology, String> {
    let p = path.display();
    let text = std::fs::read_to_string(path)
        .map_err(|e| tf!("Impossible d'ouvrir {p} : {e}", "Couldn't open {p}: {e}"))?;
    from_str(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceKind, IpConfig, Route};

    #[test]
    fn round_trip() {
        let mut t = Topology::default();
        let pc = t.add_device(DeviceKind::Pc, [10.0, 20.0]);
        let r = t.add_device(DeviceKind::Router, [100.0, 20.0]);
        t.connect(pc, r).unwrap();
        t.device_mut(pc).unwrap().interfaces[0].ip =
            Some(IpConfig::new("10.0.0.2".parse().unwrap(), 24));
        t.device_mut(pc).unwrap().gateway = Some("10.0.0.1".parse().unwrap());
        t.device_mut(r).unwrap().routes.push(Route {
            dest: "0.0.0.0".parse().unwrap(),
            prefix: 0,
            next_hop: "1.1.1.1".parse().unwrap(),
        });
        let back = from_str(&to_string(&t)).unwrap();
        assert_eq!(back, t);
        // New devices after a load still get fresh ids.
        let mut back = back;
        assert_eq!(back.add_device(DeviceKind::Pc, [0.0; 2]), 2);
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(from_str("{}").is_err());
        assert!(
            from_str(r#"{"format":"other","version":1,"topology":{"devices":[],"links":[]}}"#)
                .is_err()
        );
        assert!(
            from_str(r#"{"format":"netlix","version":99,"topology":{"devices":[],"links":[]}}"#)
                .is_err()
        );
        assert!(
            from_str(r#"{"format":"netlix","version":1,"topology":{"devices":[],"links":[]}}"#)
                .is_ok()
        );
    }
}
