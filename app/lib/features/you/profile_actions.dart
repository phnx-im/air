// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:typed_data';

import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';

final _log = Logger("ProfileActions");

/// Sets the own display name or profile picture. A failure shows a snackbar.
Future<void> setOwnProfile(
  UserCubit userCubit, {
  String? displayName,
  Uint8List? profilePicture,
}) async {
  try {
    await userCubit.setProfile(
      displayName: displayName,
      profilePicture: profilePicture,
    );
  } catch (e, stackTrace) {
    _log.severe("Failed to update profile: $e", e, stackTrace);
    showSnackBarStandalone(
      (loc) => loc.userSettingsScreen_error_updateProfile,
      tone: .danger,
    );
  }
}

/// Lets the user pick a new profile picture from the gallery and sets it.
Future<void> pickOwnProfilePicture(UserCubit userCubit) async {
  final ImagePicker picker = ImagePicker();
  // Reduce image quality to re-encode the image.
  final XFile? image = await picker.pickImage(
    source: ImageSource.gallery,
    imageQuality: 99,
  );
  final bytes = await image?.readAsBytes();

  if (bytes != null) {
    await setOwnProfile(userCubit, profilePicture: bytes);
  }
}
