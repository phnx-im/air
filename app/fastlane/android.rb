platform :android do
    desc "Build and release the app"
    lane :beta_android do |options|
      # Package name
      package_name = "ms.air"
      track = "internal"

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

          # Decode the Play Store key from the base64 string and save it to a temporary file
          playstore_key_path = "playstorekey.json"
          base64_playstore_key = ENV["ANDROID_PLAYSTORE_KEY_JSON"]
          UI.user_error!("ANDROID_PLAYSTORE_KEY_JSON environment variable is missing!") if base64_playstore_key.nil?
          File.open(playstore_key_path, "wb") do |file|
            file.write(Base64.decode64(base64_playstore_key))
          end

          # Get the previous build number
          begin
            previous_build_number = google_play_track_version_codes(
              track: track,
              package_name: package_name,
              json_key: "fastlane/" + playstore_key_path,
            )[0]
            previous_build_number = (previous_build_number || 0).to_i
          rescue
            previous_build_number = 0
          end
          current_build_number = previous_build_number + 1

          # Increment the build number in the gradle file
          increment_version_code(
            gradle_file_path: "android/app/build.gradle.kts",
            version_code: current_build_number
          )
        end

        # When not uploading to the Play Store, we just build the app as APK to
        # allow manual installation
        build_target = upload_to_play_store ? "appbundle" : "apk"

        sh "fvm flutter precache --android"
        sh "fvm flutter pub get"
        sh "fvm flutter build #{build_target} --release --target-platform android-arm64"

        if upload_to_play_store
          metadata_path = File.expand_path("../stores/android/metadata", __dir__)
          # Upload to Google Play Store
          supply(
            validate_only: false,
            release_status: "draft",
            version_code: current_build_number,
            track: track,
            aab: "build/app/outputs/bundle/release/app-release.aab",
            json_key: "fastlane/" + playstore_key_path,
            package_name: package_name,
            metadata_path: metadata_path,
          )
        end

      rescue => e
        UI.error(e)
        raise
      ensure
        # Clean up the temporary files
         if upload_to_play_store
           File.delete(keystore_path)
           File.delete(playstore_key_path)
         end
      end
    end
  end
