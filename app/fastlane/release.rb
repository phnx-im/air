require "yaml"

# Helpers for release candidates, shared by all platforms.

# Release notes shared by Google Play and the App Store, one file per app
# version. `cargo xtask cut-release` writes them on the release branch, and
# xtask/src/release_notes.rs checks the same limit.
RELEASE_NOTES_DIR = File.expand_path("../stores/common/release_notes", __dir__)

# Google Play allows 500 characters per language, the App Store 4000.
RELEASE_NOTES_MAX_LENGTH = 500

# Reads the app version from pubspec.yaml, without the build number.
def app_version
  pubspec = YAML.safe_load_file(File.expand_path("../pubspec.yaml", __dir__))
  pubspec.fetch("version").to_s.split("+").first
end

def release_notes_path(version)
  File.join(RELEASE_NOTES_DIR, "#{version}.txt")
end

# Returns the release notes of the version, or nil when it has none.
def release_notes(version)
  path = release_notes_path(version)
  return nil unless File.exist?(path)

  notes = File.read(path, encoding: "UTF-8").strip
  UI.user_error!("The release notes in #{path} are empty") if notes.empty?
  if notes.length > RELEASE_NOTES_MAX_LENGTH
    UI.user_error!("The release notes in #{path} have #{notes.length} characters, " \
                   "Google Play allows #{RELEASE_NOTES_MAX_LENGTH}")
  end
  notes
end

# Returns the release notes of the version, which a submission needs.
def required_release_notes(version)
  notes = release_notes(version)
  return notes if notes

  UI.user_error!("No release notes for #{version}, expected #{release_notes_path(version)}")
end

# Parses the build number that a submit lane gets as its `build` option.
def submitted_build_number(options)
  build_number = options[:build].to_s
  unless build_number.match?(/\A\d+\z/)
    UI.user_error!("Pass the build number of the release candidate, e.g. build:1567")
  end
  build_number.to_i
end
