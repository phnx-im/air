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
    @Default(false) bool needsUsernameOnboarding,
    @Default(false) bool isCheckingInvitationCode,
    String? usernameSuggestion,
    String? invitationCode,
  }) = _RegistrationState;

  bool get isDomainValid => _domainRegex.hasMatch(domain);
  bool get isValid =>
      isDomainValid && displayName.trim().isNotEmpty && invitationCode != null;
  String get serverUrl =>
      domain == "localhost" ? "http://$domain:8080" : "https://$domain";
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

  void setInvitationCode(String value) {
    emit(state.copyWith(invitationCode: value));
  }

  void startUsernameOnboarding(String suggestion) {
    emit(
      state.copyWith(
        needsUsernameOnboarding: true,
        usernameSuggestion: suggestion,
      ),
    );
  }

  void clearUsernameOnboarding() {
    emit(
      state.copyWith(needsUsernameOnboarding: false, usernameSuggestion: null),
    );
  }

  Future<CheckInvitationCodeError?> submitInvitationCode() async {
    if (state.invitationCode == null) {
      return const CheckInvitationCodeError(code: .missing);
    }

    emit(state.copyWith(isCheckingInvitationCode: true));

    try {
      final isValid = await checkInvitationCode(
        serverUrl: state.serverUrl,
        invitationCode: state.invitationCode!,
      );
      if (!isValid) {
        return const CheckInvitationCodeError(code: .invalid);
      }
    } catch (e) {
      _log.severe("Error when checking invitation code: ${e.toString()}");
      return CheckInvitationCodeError(code: .internal, message: e.toString());
    } finally {
      emit(state.copyWith(isCheckingInvitationCode: false));
    }

    return null;
  }

  Future<SignUpError?> signUp() async {
    if (state.invitationCode == null) {
      return const SignUpError("Invitation code is missing");
    }

    emit(state.copyWith(isSigningUp: true));

    try {
      _log.info("Registering user...");
      await _coreClient.createUser(
        state.serverUrl,
        state.displayName,
        state.avatar?.data,
        state.invitationCode!,
      );
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

final class CheckInvitationCodeError {
  const CheckInvitationCodeError({required this.code, this.message});
  final InvitationCodeError code;
  final String? message;
}

enum InvitationCodeError { missing, invalid, internal }
