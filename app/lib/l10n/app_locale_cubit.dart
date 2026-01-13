// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class AppLocaleCubit extends Cubit<Locale?> {
  AppLocaleCubit() : super(null);

  void setLocale(Locale locale) => emit(locale);

  void clearLocale() => emit(null);
}
