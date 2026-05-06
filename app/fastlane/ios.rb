require 'xcodeproj'
require 'plist'
require 'yaml'

platform :ios do
  before_all do
    @app_store_params = {
      team_id: ENV['TEAM_ID'],
      app_identifier: 'ms.air',
      app_identifier_nse: 'ms.air.nse',
    }

    key_id = ENV['APP_STORE_KEY_ID']
    issuer_id = ENV['APP_STORE_ISSUER_ID']
    key_content = ENV['APP_STORE_KEY_P8_BASE64']

    if [key_id, issuer_id, key_content].all? { |value| value && !value.empty? }
      @app_store_api_key = app_store_connect_api_key(
        key_id: key_id,
        issuer_id: issuer_id,
        key_content: key_content,
        is_key_content_base64: true,
        in_house: false
      )
    else
      @app_store_api_key = nil
      UI.message("App Store Connect credentials not available, skipping API key setup")
    end
  end

  desc "Build iOS app for TestFlight"
  lane :beta_ios do |options|
    # Set up CI
    setup_ci()
    upload_to_test_flight = options[:upload_to_test_flight]

    # Set parameters
    team_id = @app_store_params[:team_id]
    app_identifier = @app_store_params[:app_identifier]
    app_identifier_nse = @app_store_params[:app_identifier_nse]

    UI.user_error!("TEAM_ID must be provided for the beta_ios lane") if team_id.to_s.empty?

    # Load the app store connect API key
    api_key = @app_store_api_key
    UI.user_error!("App Store Connect credentials are required for the beta_ios lane") unless api_key

    # Use match for code signing
    ["development", "appstore"].each do |i|
      match(
        type: i,
        git_url: ENV['MATCH_GIT_URL'],
        git_basic_authorization: ENV['MATCH_GIT_BASIC_AUTHORIZATION'],
        git_branch: "main",
        storage_mode: "git",
        app_identifier: [app_identifier, app_identifier_nse],
        team_id: team_id,
        readonly: is_ci,
      )
    end

    # Build the app with signing
    build_ios(with_signing: upload_to_test_flight)

    # Upload the app to TestFlight if the parameter is set
    if upload_to_test_flight
      # Upload the app to TestFlight
      upload_to_testflight(
        api_key: api_key,
        app_platform: "ios",
        skip_waiting_for_build_processing: true,
        distribute_external: false,
      )

      # Find the app in ASC
      app = Spaceship::ConnectAPI::App.find(app_identifier)
      UI.user_error!("App not found: #{app_identifier}") unless app

      # Only upload metadata when the editable version is in a state we know is safe to modify.
      version_state = Spaceship::ConnectAPI::AppStoreVersion::AppVersionState
      metadata_uploadable_states = [
        version_state::PREPARE_FOR_SUBMISSION,
        version_state::DEVELOPER_REJECTED,
        version_state::REJECTED,
        version_state::METADATA_REJECTED,
        version_state::INVALID_BINARY,
      ]

      editable = app.get_edit_app_store_version
      editable_state = editable&.app_version_state || editable&.app_store_state

      if editable.nil?
        UI.important("No editable App Store version found. Skipping metadata upload.")
      elsif !metadata_uploadable_states.include?(editable_state)
        UI.important("App Store version '#{editable.version_string}' is in '#{editable_state}' state. Skipping metadata upload.")
      else
        # Upload metadata and screenshots
        UI.message("Uploading metadata and screenshots for version '#{editable.version_string}' in state '#{editable_state}'")
        upload_to_app_store(
          api_key: api_key,
          app_identifier: app_identifier,
          metadata_path: "./stores/ios/metadata",
          screenshots_path: "./stores/ios/screenshots",
          precheck_include_in_app_purchases: false,
          overwrite_screenshots: true,
          skip_binary_upload: true,
          force: true
        )
      end
    end
  end

  desc "Build app"
  lane :build_ios do |options|
    # The following is false when "with_signing" is not provided in the option
    # and true otherwise
    skip_signing = !options[:with_signing]

    build_number = sh("git rev-list --count HEAD").strip.to_i

    # Set up CI
    setup_ci()

    # Install flutter dependencies
    sh "just flutter pub get"

    # Build the app with flutter first to create the necessary ephemeral files
    sh "just flutter build ios --flavor production --config-only #{skip_signing ? '--debug --no-codesign' : '--release'} --build-number #{build_number}"

    # Install CocoaPods dependencies
    cocoapods(
      podfile: "ios/Podfile"
    )

    # Build the app
    build_app(
      workspace: "ios/Runner.xcworkspace",
      scheme: "Runner",
      configuration: skip_signing ? "Debug" : "Release",
      skip_codesigning: skip_signing,
      skip_package_ipa: skip_signing,
      skip_archive: skip_signing,
      export_method: "app-store",
    )
  end
end
