platform :ios do
  before_all do
    @app_store_api_key = apple_api_key("iOS")
  end

  desc "Build iOS app for TestFlight"
  lane :beta_ios do |options|
    apple_beta(
      :ios,
      api_key: @app_store_api_key,
      upload_to_test_flight: options[:upload_to_test_flight],
    )
  end

  desc "Build app"
  lane :build_ios do |options|
    apple_build(:ios, with_signing: options[:with_signing], api_key: @app_store_api_key)
  end
end
