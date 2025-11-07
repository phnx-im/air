// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:logging/logging.dart';

part 'registration_cubit.freezed.dart';

final _log = Logger('RegistrationCubit');

// * It consists of one or more labels separated by dots.
// * Each label can contain alphanumeric characters (A-Z, a-z, 0-9) and hyphens.
// * Labels cannot start or end with a hyphen.
// * Each label must be between 1 and 63 characters long.
final _domainRegex = RegExp(
  r'^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z0-9-]{1,63})*$',
);

@freezed
sealed class RegistrationState with _$RegistrationState {
  const RegistrationState._();

  const factory RegistrationState({
    // Domain choice screen data
    @Default('air.ms') String domain,

    // Display name/avatar screen data
    ImageData? avatar,
    @Default('') String displayName,
    @Default(false) bool isSigningUp,
  }) = _RegistrationState;

  bool get isDomainValid => _domainRegex.hasMatch(domain);
  bool get isValid => isDomainValid && displayName.trim().isNotEmpty;
}

class RegistrationCubit extends Cubit<RegistrationState> {
  RegistrationCubit({required CoreClient coreClient})
    : _coreClient = coreClient,
      super(const RegistrationState());

  final CoreClient _coreClient;

  void setDomain(String value) {
    emit(state.copyWith(domain: value));
  }

  void setAvatar(ImageData? bytes) {
    emit(state.copyWith(avatar: bytes));
  }

  void setDisplayName(String value) {
    emit(state.copyWith(displayName: value));
  }

  Future<SignUpError?> signUp() async {
    emit(state.copyWith(isSigningUp: true));

    final url =
        state.domain == "localhost"
            ? "http://${state.domain}"
            : "https://${state.domain}";

    try {
      _log.info("Registering user...");
      await _coreClient.createUser(url, state.displayName, state.avatar?.data);
    } catch (e) {
      _log.severe("Error when registering user: ${e.toString()}");
      emit(state.copyWith(isSigningUp: false));
      return SignUpError(e.toString());
    }

    emit(state.copyWith(isSigningUp: false));

    return null;
  }
}

final class SignUpError {
  const SignUpError(this.message);
  final String message;
}
