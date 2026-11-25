# Helpers shared by the ios and mac platforms. Both ship through App Store
# Connect and build through Xcode, so the lanes differ only in the values
# collected in `apple_platform` below.

# Everything that differs between the two Apple platforms. Spaceship constants
# are resolved lazily because they are not loaded yet while this file is parsed.
def apple_platform(platform)
  case platform
  when :ios
    {
      name: "iOS",
      beta_lane: "beta_ios",
      app_identifier: "ms.air",
      # The notification service and share extensions are signed alongside
      # the app.
      extension_identifiers: ["ms.air.nse", "ms.air.share"],
      # match defaults to the platform of the enclosing block, so iOS needs no
      # extra options.
      match_options: {},
      spaceship_platform: Spaceship::ConnectAPI::Platform::IOS,
      # upload_to_testflight and deliver share App Store Connect's platform
      # slugs, which are not the same vocabulary match uses.
      asc_platform: "ios",
      screenshots_dir: "ios",
      flutter_target: "ios",
      # Signing happens in the build_app step, not while writing the config.
      flutter_debug_options: ["--no-codesign"],
      xcode_dir: "ios",
    }
  when :macos
    {
      name: "macOS",
      beta_lane: "beta_macos",
      app_identifier: "ms.air",
      extension_identifiers: [],
      match_options: {
        platform: "macos",
        additional_cert_types: ["mac_installer_distribution"],
      },
      spaceship_platform: Spaceship::ConnectAPI::Platform::MAC_OS,
      asc_platform: "osx",
      screenshots_dir: "macos",
      flutter_target: "macos",
      flutter_debug_options: [],
      xcode_dir: "macos",
    }
  else
    UI.user_error!("Unknown Apple platform: #{platform}")
  end
end

# Builds an App Store Connect API key from the environment, or returns nil when
# the credentials are absent, which is the case for forks and dependabot.
def apple_api_key(platform_name)
  key_id = ENV['APP_STORE_KEY_ID']
  issuer_id = ENV['APP_STORE_ISSUER_ID']
  key_content = ENV['APP_STORE_KEY_P8_BASE64']

  unless [key_id, issuer_id, key_content].all? { |value| value && !value.empty? }
    UI.message("App Store Connect credentials not available, skipping API key setup for #{platform_name}")
    return nil
  end

  app_store_connect_api_key(
    key_id: key_id,
    issuer_id: issuer_id,
    key_content: key_content,
    is_key_content_base64: true,
    in_house: false
  )
end

# Signs and builds the app, then ships it to TestFlight when
# `upload_to_test_flight` is set. A release candidate also becomes the App Store
# draft of its version.
def apple_beta(platform, api_key:, upload_to_test_flight:, release_candidate:)
  target = apple_platform(platform)

  setup_ci()

  team_id = ENV['TEAM_ID']
  UI.user_error!("TEAM_ID must be provided for the #{target[:beta_lane]} lane") if team_id.to_s.empty?
  UI.user_error!("App Store Connect credentials are required for the #{target[:beta_lane]} lane") unless api_key

  identifiers = [target[:app_identifier]] + target[:extension_identifiers]

  ["development", "appstore"].each do |type|
    match(
      {
        type: type,
        git_url: ENV['MATCH_GIT_URL'],
        git_basic_authorization: ENV['MATCH_GIT_BASIC_AUTHORIZATION'],
        git_branch: "main",
        storage_mode: "git",
        app_identifier: identifiers,
        team_id: team_id,
        readonly: is_ci,
      }.merge(target[:match_options])
    )
  end

  apple_build(platform, with_signing: upload_to_test_flight, api_key: api_key)

  return unless upload_to_test_flight

  upload_to_testflight(
    api_key: api_key,
    app_platform: target[:asc_platform],
    skip_waiting_for_build_processing: true,
    distribute_external: false,
  )

  # main has the listing of unreleased features, which is why its builds stay
  # in TestFlight.
  apple_prepare_draft(platform, api_key: api_key) if release_candidate
end

# Builds the app, signing it only when `with_signing` is set. Unsigned builds
# are what forks and dependabot run, so they must not need any credentials.
def apple_build(platform, with_signing:, api_key:)
  target = apple_platform(platform)
  skip_signing = !with_signing

  unless skip_signing
    UI.user_error!("TEAM_ID must be provided when with_signing is true") if ENV['TEAM_ID'].to_s.empty?
    UI.user_error!("App Store Connect credentials are required when with_signing is true") unless api_key
  end

  build_number = sh("just build-number").strip.to_i

  setup_ci()

  sh "just flutter pub get"

  # Build with flutter first to create the necessary ephemeral files
  flutter_options = skip_signing ? ["--debug"] + target[:flutter_debug_options] : ["--release"]
  sh "just flutter build #{target[:flutter_target]} --flavor production " \
     "--config-only #{flutter_options.join(' ')} --build-number=#{build_number}"

  xcode_options = {
    workspace: "#{target[:xcode_dir]}/Runner.xcworkspace",
    scheme: "Runner",
    configuration: skip_signing ? "Debug" : "Release",
    skip_codesigning: skip_signing,
    skip_archive: skip_signing,
    export_method: "app-store",
  }

  # gym exposes a different skip option per package format.
  case platform
  when :ios
    build_app(xcode_options.merge(skip_package_ipa: skip_signing))
  when :macos
    build_mac_app(xcode_options.merge(skip_package_pkg: skip_signing))
  end
end

# States in which we are willing to modify the editable version.
# `get_edit_app_store_version` already filters to editable states, so in
# practice this list differs only by excluding WAITING_FOR_REVIEW: we leave a
# version that has already been submitted alone.
def apple_metadata_uploadable_states
  state = Spaceship::ConnectAPI::AppStoreVersion::AppVersionState
  [
    state::PREPARE_FOR_SUBMISSION,
    state::DEVELOPER_REJECTED,
    state::REJECTED,
    state::METADATA_REJECTED,
    state::INVALID_BINARY,
  ]
end

# Explains why the version cannot be the App Store draft, or returns nil when
# it can.
#
# App Store Connect has one editable version per platform. deliver renames it
# to the version it uploads, or creates it when there is none.
def apple_draft_blocker(target, version)
  app = Spaceship::ConnectAPI::App.find(target[:app_identifier])
  UI.user_error!("App not found: #{target[:app_identifier]}") unless app

  # get_edit_app_store_version defaults to the iOS platform, so the platform is
  # always passed explicitly.
  editable = app.get_edit_app_store_version(platform: target[:spaceship_platform])
  return apple_new_version_blocker(app, target, version) if editable.nil?

  state = editable.app_version_state || editable.app_store_state
  unless apple_metadata_uploadable_states.include?(state)
    return "App Store version '#{editable.version_string}' is in '#{state}' state"
  end
  # A build of an older release line, such as a hotfix, leaves the draft of a
  # newer one alone.
  if Gem::Version.new(editable.version_string) > Gem::Version.new(version)
    return "App Store Connect prepares the newer version '#{editable.version_string}'"
  end

  nil
end

# States of a version that is neither editable nor live. App Store Connect
# creates no new version of the platform while one is in such a state.
def apple_in_flight_states
  state = Spaceship::ConnectAPI::AppStoreVersion::AppVersionState
  [
    state::READY_FOR_REVIEW,
    state::WAITING_FOR_EXPORT_COMPLIANCE,
    state::IN_REVIEW,
    state::ACCEPTED,
    state::PENDING_APPLE_RELEASE,
    state::PENDING_DEVELOPER_RELEASE,
  ]
end

# Explains why deliver cannot create the version, or returns nil when it can.
def apple_new_version_blocker(app, target, version)
  platform = target[:spaceship_platform]
  in_flight = app.get_app_store_versions(
    filter: { appVersionState: apple_in_flight_states.join(","), platform: platform }
  ).first
  if in_flight
    return "App Store version '#{in_flight.version_string}' is in " \
           "'#{in_flight.app_version_state}' state"
  end

  existing = app.get_app_store_versions(
    filter: { versionString: version, platform: platform }
  ).first
  return nil unless existing

  state = existing.app_version_state || existing.app_store_state
  "App Store version '#{version}' is already in '#{state}' state"
end

# Makes the version of this build the App Store draft, with the store listing
# and release notes of this branch. Apple reviews nothing before apple_submit
# sends the draft.
def apple_prepare_draft(platform, api_key:)
  target = apple_platform(platform)
  version = app_version
  blocker = apple_draft_blocker(target, version)
  if blocker
    UI.important("#{blocker}. Skipping the #{target[:name]} App Store draft.")
    return
  end

  notes = release_notes(version)
  unless notes
    UI.important("No release notes for #{version} yet, expected #{release_notes_path(version)}")
  end
  apple_deliver(target, api_key: api_key, version: version, notes: notes)
end

# Attaches the build to the App Store draft of its version and sends the draft
# for review, with the store listing and release notes of this branch.
def apple_submit(platform, api_key:, options:)
  target = apple_platform(platform)
  UI.user_error!("App Store Connect credentials are required to submit") unless api_key
  build_number = submitted_build_number(options)
  version = app_version
  notes = required_release_notes(version)
  blocker = apple_draft_blocker(target, version)
  UI.user_error!("#{blocker}. Build #{build_number} cannot go to review now.") if blocker

  apple_deliver(
    target, api_key: api_key, version: version, notes: notes, submit_build: build_number
  )
end

# Publishes the approved App Store version of the build to all users.
#
# apple_submit turns automatic release off, which leaves an approved version
# waiting for this. A version that is live already with the build passes, which
# lets a partly failed `cargo xtask publish-release` run again for all
# platforms.
def apple_publish(platform, api_key:, options:)
  target = apple_platform(platform)
  UI.user_error!("App Store Connect credentials are required to publish") unless api_key
  build_number = submitted_build_number(options)
  app = Spaceship::ConnectAPI::App.find(target[:app_identifier])
  UI.user_error!("App not found: #{target[:app_identifier]}") unless app

  state = Spaceship::ConnectAPI::AppStoreVersion::AppVersionState
  pending = app.get_app_store_versions(
    filter: {
      appVersionState: state::PENDING_DEVELOPER_RELEASE,
      platform: target[:spaceship_platform],
    }
  ).first
  if pending.nil?
    apple_check_live(app, target, build_number)
    return
  end

  pending_build = pending.build&.version
  unless pending_build == build_number.to_s
    UI.user_error!("App Store version '#{pending.version_string}' waits for release with " \
                   "build #{pending_build || 'none'}, not #{build_number}")
  end
  pending.create_app_store_version_release_request
  UI.success("Published #{target[:name]} App Store version '#{pending.version_string}' " \
             "(build #{build_number})")
end

# Passes when the live App Store version has the build, and explains otherwise
# why no version waits for release.
def apple_check_live(app, target, build_number)
  live = app.get_live_app_store_version(platform: target[:spaceship_platform])
  if live && live.build&.version == build_number.to_s
    UI.important("#{target[:name]} App Store version '#{live.version_string}' " \
                 "(build #{build_number}) is live already")
    return
  end

  live_text = "Nothing is live"
  live_text = "Live is '#{live.version_string}' (build #{live.build&.version})" if live
  UI.user_error!("No #{target[:name]} App Store version waits for release. #{live_text}. " \
                 "App Store Connect shows whether build #{build_number} is still in review.")
end

# Uploads the store listing and release notes to the App Store version. With
# `submit_build`, it also attaches that build and sends the version for review.
def apple_deliver(target, api_key:, version:, notes:, submit_build: nil)
  options = {
    api_key: api_key,
    app_identifier: target[:app_identifier],
    platform: target[:asc_platform],
    app_version: version,
    # Both Apple stores list the same text, which is why they share one
    # metadata tree. Only the screenshots are per-platform.
    metadata_path: "./stores/apple/metadata",
    screenshots_path: "./stores/#{target[:screenshots_dir]}/screenshots",
    # The notes are in English. deliver leaves them unchanged when this is nil.
    release_notes: notes ? { "en-US" => notes } : nil,
    # An approved version waits for a manual release, which then reaches all
    # users at once.
    automatic_release: false,
    phased_release: false,
    precheck_include_in_app_purchases: false,
    overwrite_screenshots: true,
    skip_binary_upload: true,
    force: true,
  }
  if submit_build
    options[:build_number] = submit_build.to_s
    options[:submit_for_review] = true
  end
  upload_to_app_store(**options)
end
