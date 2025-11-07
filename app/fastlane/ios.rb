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

    @app_store_api_key = app_store_connect_api_key(
      key_id: ENV['APP_STORE_KEY_ID'],
      issuer_id: ENV['APP_STORE_ISSUER_ID'],
      key_content: ENV['APP_STORE_KEY_P8_BASE64'],
      is_key_content_base64: true,
      in_house: false
    )
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
  
    # Load the app store connect API key
    api_key = @app_store_api_key
  
    # Read app version from pubspec.yaml
    pubspec = YAML.load_file("../pubspec.yaml")
    app_version = (pubspec['version'] || '').to_s.split('+').first

    # Determine build number
    build_number = if options[:build_number]
                    options[:build_number].to_i
                  else
                    latest_testflight_build_number(
                      version: app_version,
                      api_key: api_key,
                      app_identifier: app_identifier
                    ) + 1
                  end
  
    increment_build_number(
      xcodeproj: "ios/Runner.xcodeproj",
      build_number: build_number,
    )
  
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

      # Check for any blocking store version states
      pending_release = app.get_pending_release_app_store_version   # PENDING_APPLE_RELEASE / PENDING_DEVELOPER_RELEASE
      in_review       = app.get_in_review_app_store_version         # IN_REVIEW
      editable        = app.get_edit_app_store_version              # PREPARE_FOR_SUBMISSION / WAITING_FOR_REVIEW / etc.

      blocking = [pending_release, in_review, editable].compact

      if blocking.any?
        UI.important("There is a pending App Store version in a blocking state. Skipping metadata upload.")
      else
        # Upload metadata and screenshots
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
  
    # Set up CI
    setup_ci()

    # Install flutter dependencies
    sh "flutter pub get"

    # Build the app with flutter first to create the necessary ephemeral files
    sh "flutter build ios --config-only #{skip_signing ? '--debug' : '--release'}"
  
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
