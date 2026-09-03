// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Light or dark, overriding the platform. Prototype tooling for the theme
/// showcase, not persisted.
class ThemeModeCubit extends Cubit<ThemeMode> {
  ThemeModeCubit() : super(ThemeMode.system);

  /// Flips away from [current], the brightness the app is showing right now.
  void toggle(Brightness current) =>
      emit(current == Brightness.dark ? ThemeMode.light : ThemeMode.dark);
}
