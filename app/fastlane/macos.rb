platform :mac do
  before_all do
    @app_store_api_key = apple_api_key("macOS")
  end

  desc "Build macOS app for TestFlight"
  lane :beta_macos do |options|
    apple_beta(
      :macos,
      api_key: @app_store_api_key,
      upload_to_test_flight: options[:upload_to_test_flight],
      release_candidate: options[:release_candidate],
    )
  end

  desc "Send a release candidate to App Store review, with the store listing and release notes"
  lane :submit_macos do |options|
    apple_submit(:macos, api_key: @app_store_api_key, options: options)
  end

  desc "Publish the approved App Store version of a release candidate to all users"
  lane :publish_macos do |options|
    apple_publish(:macos, api_key: @app_store_api_key, options: options)
  end

  desc "Build macOS app"
  lane :build_macos do |options|
    apple_build(:macos, with_signing: options[:with_signing], api_key: @app_store_api_key)
  end
end
