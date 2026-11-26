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
      release_candidate: options[:release_candidate],
    )
  end

  desc "Send a release candidate to App Store review, with the store listing and release notes"
  lane :submit_ios do |options|
    apple_submit(:ios, api_key: @app_store_api_key, options: options)
  end

  desc "Publish the approved App Store version of a release candidate to all users"
  lane :publish_ios do |options|
    apple_publish(:ios, api_key: @app_store_api_key, options: options)
  end

  desc "Build app"
  lane :build_ios do |options|
    apple_build(:ios, with_signing: options[:with_signing], api_key: @app_store_api_key)
  end
end
