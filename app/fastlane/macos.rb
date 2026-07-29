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
    )
  end

  desc "Build macOS app"
  lane :build_macos do |options|
    apple_build(:macos, with_signing: options[:with_signing], api_key: @app_store_api_key)
  end
end
