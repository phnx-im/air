// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:logging/logging.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uuid/uuid.dart';
import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';

final _log = Logger('CoreClient');

Future<String> dbPath() async {
  final String path;

  if (Platform.isAndroid || Platform.isIOS) {
    path = await getDatabaseDirectoryMobile();
  } else if (Platform.isLinux) {
    // This corresponds to the $XDG_DATA_HOME/<AppName> directory
    final directory = await getApplicationSupportDirectory();
    path = directory.path;
  } else {
    final directory = await getApplicationDocumentsDirectory();
    path = directory.path;
  }
  return path;
}

class CoreClient {
  static final CoreClient _coreClient = CoreClient._internal();

  factory CoreClient() {
    return _coreClient;
  }

  CoreClient._internal();

  User? _user;

  final StreamController<User?> _userController = StreamController<User?>();

  User? get maybeUser => _user;

  Stream<User?> get userStream => _userController.stream;

  User get user => _user!;

  set user(User? user) {
    _userController.add(user);
    _user = user;
  }

  void logout() {
    user = null;
  }

  // used in dev settings
  Future<void> deleteAllDatabases() async {
    await deleteDatabases(dbPath: await dbPath());
    _userController.add(null);
    _user = null;
  }

  /// Tears this device down, stopping the outbound service first.
  Future<void> deleteCurrentDatabase() async {
    await deleteClientDatabase(
      user: user,
      dbPath: await dbPath(),
      clientRecordId: user.clientRecordId,
    );
    _userController.add(null);
    _user = null;
  }

  // used in app initialization
  Future<void> loadDefaultUser() async {
    user = await User.loadDefault(path: await dbPath()).onError((
      error,
      stackTrace,
    ) {
      _log.severe("Error loading default user $error");
      return null;
    });
  }

  // used in registration cubit
  Future<void> createUser(
    String domain,
    String displayName,
    Uint8List? profilePicture,
    RegistrationChallenge? challenge,
  ) async {
    final pushToken = await getPushToken();

    user = await User.newInstance(
      domain: domain,
      path: await dbPath(),
      pushToken: pushToken,
      displayName: displayName,
      profilePicture: profilePicture,
      challenge: challenge,
    );

    _log.info("User registered: ${user.userId}");
  }

  Future<void> loadUser({required UuidValue clientRecordId}) async {
    user = await User.load(
      dbPath: await dbPath(),
      clientRecordId: clientRecordId,
    );
  }

  Future<void> refreshPushToken() async {
    final currentUser = _user;
    if (currentUser == null) {
      return;
    }

    final pushToken = await getPushToken();
    if (pushToken == null) {
      return;
    }

    try {
      await currentUser.updatePushToken(pushToken);
    } catch (error, stackTrace) {
      _log.severe("Failed to update push token", error, stackTrace);
    }
  }
}
