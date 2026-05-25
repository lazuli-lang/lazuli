//! `lazuli inspect` projection of `Lazurite.toml` — borrow-friendly
//! shapes that serialise the manifest into the canonical JSON output
//! without dragging the parse-time types into the public surface.

use serde::Serialize;

use super::frontends::FrontendTarget;
use super::generate::Generate;
use super::migrations::{DevOverrides, Migrations, Seeds};
use super::project::{Lazurite, LazuliPin, Project};

#[derive(Debug, Serialize)]
pub struct InspectManifest<'a> {
    pub origin: &'static str,
    pub project: &'a Project,
    pub lazuli: &'a LazuliPin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lazurite: Option<&'a Lazurite>,
    pub plugins: Vec<InspectPlugin<'a>>,
    pub generate: &'a Generate,
    pub frontends: Vec<InspectFrontend<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrations: Option<&'a Migrations>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeds: Option<&'a Seeds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<&'a DevOverrides>,
}

#[derive(Debug, Serialize)]
pub struct InspectPlugin<'a> {
    #[serde(rename = "ref")]
    pub plugin_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<&'a str>,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct InspectFrontend<'a> {
    pub name: &'a str,
    pub target: &'a FrontendTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'a str>,
    pub out: &'a str,
    pub audiences: &'a [String],
}
