require "tmpdir"

# Makes edit commits fail while other changes are in review on Google Play.
#
# By default, a commit that sends changes for review cancels the review in
# progress and submits everything again. Google counts the review time from the
# last submission. supply has no option for this, which is why we change the
# default of the Google API client.
module ErrorIfChangesInReview
  def commit_edit(package_name, edit_id, changes_in_review_behavior: "ERROR_IF_IN_REVIEW",
                  **options, &block)
    super
  rescue Google::Apis::ClientError => e
    raise unless changes_in_review_error?(e)

    UI.user_error!("Google Play already has changes in review. Wait for the review to finish, " \
                   "or remove the changes from review in the Play Console.")
  end

  private

  def changes_in_review_error?(error)
    details = JSON.parse(error.body.to_s).dig("error", "details") || []
    details.any? { |detail| detail["reason"] == "CHANGES_ALREADY_IN_REVIEW" }
  rescue JSON::ParserError
    false
  end
end

# Applies ErrorIfChangesInReview to the Google API client that supply uses.
def refuse_play_commits_in_review
  require "google/apis/androidpublisher_v3"
  Google::Apis::AndroidpublisherV3::AndroidPublisherService.prepend(ErrorIfChangesInReview)
end

# Decodes the Play Store service account key from the environment.
def play_store_key_data
  base64_key = ENV["ANDROID_PLAYSTORE_KEY_JSON"]
  UI.user_error!("ANDROID_PLAYSTORE_KEY_JSON environment variable is missing!") if base64_key.nil?
  Base64.decode64(base64_key)
end

# Copies the Play Store listing into `target`, with the files its symlinks
# point to. The links are relative and would dangle in the copy.
def copy_play_listing(target)
  listing_path = File.expand_path("../stores/android/metadata", __dir__)
  Dir.glob("**/*", base: listing_path).each do |entry|
    source = File.join(listing_path, entry)
    next if File.directory?(source)

    destination = File.join(target, entry)
    FileUtils.mkdir_p(File.dirname(destination))
    FileUtils.cp(source, destination)
  end
end

platform :android do
    desc "Build and release the app"
    lane :beta_android do |options|
      # Package name
      package_name = "ms.air"

      # Builds from main go to internal testers. A release candidate goes to
      # production as a draft and waits there until submit_android sends it for
      # review.
      release_candidate = options[:release_candidate]
      track = release_candidate ? "production" : "internal"

      # Determine if we should deploy to the Play Store
      upload_to_play_store = options[:upload_to_play_store]

      # We need to wrap the whole process in a begin/rescue block to ensure that we clean up the temporary files
      begin
        # We prepare the keystore and the Play Store key
        if upload_to_play_store
          # Decode the keystore from the base64 string and save it to a temporary file
          keystore_path = "release-key.jks"
          base64_keystore = ENV["ANDROID_KEYSTORE_BASE64"]
          UI.user_error!("ANDROID_KEYSTORE_BASE64 environment variable is missing!") if base64_keystore.nil?
          File.open(keystore_path, "wb") do |file|
            file.write(Base64.decode64(base64_keystore))
          end

          # Prepare the signing properties
          File.open("../android/key.properties", "w") do |file|
            file.write("storeFile=#{File.expand_path(keystore_path)}\n")
            file.write("storePassword=#{ENV["ANDROID_KEY_PASSWORD"]}\n")
            file.write("keyAlias=upload\n")
            file.write("keyPassword=#{ENV["ANDROID_KEY_PASSWORD"]}\n")
          end

          play_store_key = play_store_key_data
        end

        current_build_number = sh("just build-number").strip.to_i

        # When not uploading to the Play Store, we just build the app as APK to
        # allow manual installation
        build_target = upload_to_play_store ? "appbundle" : "apk"
        flavor = upload_to_play_store ? "production" : "staging"

        sh "just flutter precache --android"
        sh "just flutter pub get"
        sh "just flutter build #{build_target} --flavor #{flavor} --release --target-platform android-arm64 --build-number=#{current_build_number}"

        if upload_to_play_store
          # A new draft replaces the releases on the production track. We let it
          # replace an earlier draft, but not a release candidate that is in
          # review.
          refuse_play_commits_in_review if release_candidate
          # Upload to Google Play Store
          supply(
            validate_only: false,
            release_status: release_candidate ? "draft" : "completed",
            version_code: current_build_number,
            track: track,
            aab: "build/app/outputs/bundle/productionRelease/app-production-release.aab",
            json_key_data: play_store_key,
            package_name: package_name,
            # Google reviews store listing changes as soon as they are committed.
            # The listing goes out with the release in submit_android instead.
            skip_upload_metadata: true,
            skip_upload_images: true,
            skip_upload_screenshots: true,
          )
        end

      rescue => e
        UI.error(e)
        raise
      ensure
        # A missing secret stops the lane before the keystore file exists.
        File.delete(keystore_path) if keystore_path && File.exist?(keystore_path)
      end
    end

    desc "Send a release candidate on the production track for review, with the store " \
         "listing and release notes"
    lane :submit_android do |options|
      version_code = submitted_build_number(options)
      notes = required_release_notes(app_version)
      play_store_key = play_store_key_data

      refuse_play_commits_in_review
      # supply reads release notes only from changelogs/<version code>.txt in
      # the metadata dir. We add that file to a copy of the listing, which
      # keeps the checkout unchanged.
      Dir.mktmpdir do |metadata_path|
        copy_play_listing(metadata_path)
        changelogs_path = File.join(metadata_path, "en-US", "changelogs")
        FileUtils.mkdir_p(changelogs_path)
        File.write(File.join(changelogs_path, "#{version_code}.txt"), notes)

        # Completes the draft and uploads the store listing and release notes
        # of this branch in one commit, which Google reviews as a whole. supply
        # updates an existing release only when it gets a rollout, and 1 means
        # all users. Managed publishing in the Play Console holds the approved
        # release until someone publishes it there, as the API cannot.
        supply(
          track: "production",
          version_code: version_code,
          rollout: "1",
          skip_upload_apk: true,
          skip_upload_aab: true,
          json_key_data: play_store_key,
          package_name: "ms.air",
          metadata_path: metadata_path,
          sync_image_upload: true,
        )
      end
    end
  end
