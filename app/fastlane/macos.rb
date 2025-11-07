require 'xcodeproj'
require 'plist'
require 'yaml'

platform :mac do
    desc "Build macOS app for TestFlight"
    lane :beta_macos do |options|
      # Set up CI
      setup_ci()
      upload_to_test_flight = options[:upload_to_test_flight]

      # Set parameters
      key_id = ENV['APP_STORE_KEY_ID']
      issuer_id = ENV['APP_STORE_ISSUER_ID']
      key_content = ENV['APP_STORE_KEY_P8_BASE64']
      team_id = ENV['TEAM_ID']
      matchType = "appstore"
      app_identifier = "ms.air"
    
      # Load the app store connect API key
      api_key = app_store_connect_api_key(
        key_id: key_id,
        issuer_id: issuer_id,
        key_content: key_content,
        is_key_content_base64: true,
        in_house: false
      )

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
                        platform: "osx",
                        app_identifier: app_identifier
                      ) + 1
                    end
    
      increment_build_number(
        xcodeproj: "macos/Runner.xcodeproj",
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
          app_identifier: [app_identifier],
          team_id: team_id,
          readonly: is_ci,
          platform: "macos",
          additional_cert_types: ["mac_installer_distribution"]
        )
      end
  
      # Build the app with signing
      build_macos(with_signing: upload_to_test_flight)

      # Upload the app to TestFlight if the parameter is set
      if upload_to_test_flight
        upload_to_testflight(
          api_key: api_key,
          app_platform: "osx", 
          skip_waiting_for_build_processing: true,
          distribute_external: false,
        )
      end
    end
  
    desc "Build macOS app"
    lane :build_macos do |options|
      # The following is false when "with_signing" is not provided in the oprion and true otherwise
      skip_signing = !options[:with_signing]
    
      # Set up CI
      setup_ci()
  
      # Install flutter dependencies
      sh "flutter pub get"

      # Build the app with flutter first to create the necessary ephemeral files
      sh "flutter build macos --config-only #{skip_signing ? '--debug' : '--release'}"
    
      # Install CocoaPods dependencies
      cocoapods(
        podfile: "macos/Podfile"
      )
  
      # Build the app
      build_mac_app(
        workspace: "macos/Runner.xcworkspace", 
        scheme: "Runner",
        configuration: skip_signing ? "Debug" : "Release",
        skip_codesigning: skip_signing,
        skip_archive: skip_signing,
        skip_package_pkg: skip_signing,
        export_method: "app-store",
      )
    end
  end
