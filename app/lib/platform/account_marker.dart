// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:logging/logging.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

// The marker lives in the app sandbox, which iOS backs up and transfers, unlike
// the databases. Finding it without an account means the data stayed on another
// device. Only used on iOS.

final _log = Logger('AccountMarker');

Future<File> _markerFile() async => File(
  p.join((await getApplicationSupportDirectory()).path, 'account_marker'),
);

Future<void> writeAccountMarker() async {
  if (!Platform.isIOS) return;
  try {
    final file = await _markerFile();
    if (!await file.exists()) {
      await file.create(recursive: true);
    }
  } catch (error, stackTrace) {
    _log.warning('Failed to write account marker: $error', error, stackTrace);
  }
}

Future<void> deleteAccountMarker() async {
  if (!Platform.isIOS) return;
  try {
    final file = await _markerFile();
    if (await file.exists()) {
      await file.delete();
    }
  } catch (error, stackTrace) {
    _log.warning('Failed to delete account marker: $error', error, stackTrace);
  }
}

Future<bool> hasAccountMarker() async {
  if (!Platform.isIOS) return false;
  try {
    return await (await _markerFile()).exists();
  } catch (error, stackTrace) {
    _log.warning('Failed to read account marker: $error', error, stackTrace);
    return false;
  }
}
