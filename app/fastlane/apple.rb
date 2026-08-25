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

# Signs and builds the app, then ships it to TestFlight and refreshes the store
# listing when `upload_to_test_flight` is set.
def apple_beta(platform, api_key:, upload_to_test_flight:)
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

  apple_upload_metadata(
    api_key: api_key,
    app_identifier: target[:app_identifier],
    platform: platform,
  )
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

  cocoapods(
    podfile: "#{target[:xcode_dir]}/Podfile"
  )

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

# Uploads store metadata and screenshots for a build that is already on App
# Store Connect.
def apple_upload_metadata(api_key:, app_identifier:, platform:)
  target = apple_platform(platform)

  app = Spaceship::ConnectAPI::App.find(app_identifier)
  UI.user_error!("App not found: #{app_identifier}") unless app

  # get_edit_app_store_version defaults to the iOS platform, so the platform is
  # always passed explicitly.
  editable = app.get_edit_app_store_version(platform: target[:spaceship_platform])
  editable_state = editable&.app_version_state || editable&.app_store_state

  if editable.nil?
    UI.important("No editable App Store version found. Skipping metadata upload.")
    return
  end

  unless apple_metadata_uploadable_states.include?(editable_state)
    UI.important("App Store version '#{editable.version_string}' is in '#{editable_state}' state. Skipping metadata upload.")
    return
  end

  UI.message("Uploading metadata and screenshots for version '#{editable.version_string}' in state '#{editable_state}'")
  upload_to_app_store(
    api_key: api_key,
    app_identifier: app_identifier,
    platform: target[:asc_platform],
    # Both Apple stores list the same text, so they share one metadata tree.
    # Only the screenshots are per-platform.
    metadata_path: "./stores/apple/metadata",
    screenshots_path: "./stores/#{target[:screenshots_dir]}/screenshots",
    precheck_include_in_app_purchases: false,
    overwrite_screenshots: true,
    skip_binary_upload: true,
    force: true
  )
end
