// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/color_theme.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The selected color theme. Prototype: in memory only, resets on restart.
class ColorThemeCubit extends Cubit<ColorTheme> {
  ColorThemeCubit() : super(builtinColorThemes.first);

  void select(ColorTheme theme) => emit(theme);
}
