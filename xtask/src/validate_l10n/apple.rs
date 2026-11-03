// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Checks that the Apple projects declare exactly the ARB languages.
//!
//! The App Store derives an app's languages from the bundle's `.lproj`
//! folders and `CFBundleLocalizations`, not from Flutter's ARB files.

use std::{collections::BTreeSet, fs};

use camino::Utf8Path;
use regex::Regex;

use super::Report;

const PLATFORMS: &[&str] = &["ios/Runner", "macos/Runner"];
const STRINGS_FILE: &str = "InfoPlist.strings";
const BASE: &str = "Base";

pub(super) fn check(project_root: &Utf8Path, arb_file_names: &[&str], report: &mut Report) {
    let expected: BTreeSet<String> = arb_file_names
        .iter()
        .filter_map(|name| apple_locale(name))
        .collect();
    for platform in PLATFORMS {
        let dir = project_root.join(platform);
        check_info_plist(&dir, platform, &expected, report);
        check_lproj_dirs(&dir, platform, &expected, report);
        check_pbxproj(&dir, platform, &expected, report);
    }
}

/// Maps `app_pt_PT.arb` to `pt-PT`. Flutter's bare `zh` is Simplified.
fn apple_locale(arb_file_name: &str) -> Option<String> {
    let tag = arb_file_name
        .strip_prefix("app_")?
        .strip_suffix(".arb")?
        .replace('_', "-");
    Some(if tag == "zh" {
        "zh-Hans".to_owned()
    } else {
        tag
    })
}

fn check_info_plist(
    dir: &Utf8Path,
    platform: &str,
    expected: &BTreeSet<String>,
    report: &mut Report,
) {
    let file = format!("{platform}/Info.plist");
    let Ok(plist) = fs::read_to_string(dir.join("Info.plist")) else {
        report.push(&file, "-", "cannot read Info.plist");
        return;
    };
    let array = Regex::new(r"(?s)<key>CFBundleLocalizations</key>\s*<array>(.*?)</array>")
        .expect("valid regex")
        .captures(&plist)
        .map(|captures| captures[1].to_owned());
    let Some(array) = array else {
        report.push(&file, "-", "CFBundleLocalizations array is missing");
        return;
    };
    let actual = Regex::new(r"<string>([^<]*)</string>")
        .expect("valid regex")
        .captures_iter(&array)
        .map(|captures| captures[1].trim().to_owned())
        .collect();
    report_diff(
        report,
        &file,
        expected,
        &actual,
        "add it to CFBundleLocalizations",
        "remove it from CFBundleLocalizations",
    );
}

fn check_lproj_dirs(
    dir: &Utf8Path,
    platform: &str,
    expected: &BTreeSet<String>,
    report: &mut Report,
) {
    let file = format!("{platform}/*.lproj");
    let mut present = BTreeSet::new();
    let mut with_strings = BTreeSet::new();
    if let Ok(entries) = dir.read_dir_utf8() {
        for entry in entries.flatten() {
            let Some(locale) = entry.file_name().strip_suffix(".lproj") else {
                continue;
            };
            if locale == BASE {
                continue;
            }
            present.insert(locale.to_owned());
            if entry.path().join(STRINGS_FILE).is_file() {
                with_strings.insert(locale.to_owned());
            }
        }
    }
    for locale in expected {
        if !with_strings.contains(locale) {
            report.push(
                &file,
                locale,
                format!("missing {locale}.lproj/{STRINGS_FILE}, add it"),
            );
        }
    }
    for locale in present.difference(expected) {
        report.push(
            &file,
            locale,
            format!("{locale}.lproj has no ARB file, remove the directory"),
        );
    }
}

fn check_pbxproj(dir: &Utf8Path, platform: &str, expected: &BTreeSet<String>, report: &mut Report) {
    let file = format!("{platform}.xcodeproj/project.pbxproj");
    let Ok(project) = fs::read_to_string(format!("{dir}.xcodeproj/project.pbxproj")) else {
        report.push(&file, "-", "cannot read project.pbxproj");
        return;
    };

    let regions = Regex::new(r"(?s)knownRegions = \((.*?)\);")
        .expect("valid regex")
        .captures(&project)
        .map(|captures| captures[1].to_owned());
    match regions {
        None => report.push(&file, "-", "knownRegions is missing"),
        Some(regions) => {
            let mut actual: BTreeSet<String> = regions
                .split(',')
                .map(|region| region.trim().trim_matches('"').to_owned())
                .filter(|region| !region.is_empty())
                .collect();
            if !actual.remove(BASE) {
                report.push(&file, BASE, "add it to knownRegions");
            }
            report_diff(
                report,
                &file,
                expected,
                &actual,
                "add it to knownRegions",
                "remove it from knownRegions",
            );
        }
    }

    let reference = Regex::new(&format!(
        r#"path = "?([A-Za-z-]+)\.lproj/{}"?;"#,
        regex::escape(STRINGS_FILE)
    ))
    .expect("valid regex");
    let referenced: BTreeSet<String> = reference
        .captures_iter(&project)
        .map(|captures| captures[1].to_owned())
        .collect();
    report_diff(
        report,
        &file,
        expected,
        &referenced,
        &format!("add a {STRINGS_FILE} file reference for it"),
        &format!("remove its {STRINGS_FILE} file reference"),
    );
}

fn report_diff(
    report: &mut Report,
    file: &str,
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
    missing_hint: &str,
    extra_hint: &str,
) {
    for locale in expected.difference(actual) {
        report.push(file, locale, format!("missing, {missing_hint}"));
    }
    for locale in actual.difference(expected) {
        report.push(file, locale, format!("has no ARB file, {extra_hint}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_arb_names_to_apple_locales() {
        assert_eq!(apple_locale("app_en.arb").as_deref(), Some("en"));
        assert_eq!(apple_locale("app_pt_PT.arb").as_deref(), Some("pt-PT"));
        assert_eq!(apple_locale("app_zh.arb").as_deref(), Some("zh-Hans"));
        assert_eq!(apple_locale("app_zh_Hant.arb").as_deref(), Some("zh-Hant"));
    }
}
