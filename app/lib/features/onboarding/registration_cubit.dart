// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

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

/// The challenge kinds this build can collect an answer for.
const _supportedChallenges = {ChallengeKind.invitationCode};

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
    @Default(false) bool isCheckingInvitationCode,
    String? invitationCode,

    /// What the server said about signing up with it, once it was asked.
    RegistrationInfo? registrationInfo,
  }) = _RegistrationState;

  /// Whether the flow collects an invitation code.
  ///
  /// Also true before the server has answered, because that is what every
  /// server that gates registration accepts, and what servers older than the
  /// question do.
  bool get invitationCodeRequired {
    final info = registrationInfo;
    if (info == null) return true;
    return info.challengeRequired &&
        info.acceptedChallenges.contains(ChallengeKind.invitationCode);
  }

  /// Whether the server wants a challenge of a kind this build cannot collect.
  bool get challengeUnsupported {
    final info = registrationInfo;
    if (info == null) return false;
    return info.challengeRequired &&
        !info.acceptedChallenges.any(_supportedChallenges.contains);
  }

  bool get isDomainValid => _domainRegex.hasMatch(domain);
  bool get isValid =>
      isDomainValid &&
      displayName.trim().isNotEmpty &&
      (!invitationCodeRequired || invitationCode != null);
}

/// How long typing settles before the new server is asked about.
const _domainSettleDelay = Duration(milliseconds: 500);

/// How long the server gets to answer before the flow goes with its fallback.
const _registrationInfoTimeout = Duration(seconds: 3);

class RegistrationCubit extends Cubit<RegistrationState> {
  RegistrationCubit({required this._coreClient})
    : super(const RegistrationState());

  final CoreClient _coreClient;

  Timer? _domainSettleTimer;

  @override
  Future<void> close() {
    _domainSettleTimer?.cancel();
    return super.close();
  }

  void setDomain(String value) {
    // The old answer stays on screen until the new server answers, so the
    // steps never change under a hand that is still typing. A stale answer is
    // harmless: submitting against the wrong steps ends in ChallengeRequired,
    // which reroutes the flow.
    emit(state.copyWith(domain: value));
    // The field reports every keystroke, so the question waits for the typing
    // to stop.
    _domainSettleTimer?.cancel();
    _domainSettleTimer = Timer(_domainSettleDelay, loadRegistrationInfo);
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

  /// Asks the server what signing up with it needs, so the flow knows which
  /// steps to draw.
  ///
  /// A server that cannot answer leaves the flow asking for an invitation code,
  /// which is what such a server registers users with. A half-typed domain is
  /// not asked at all, and one that takes too long counts as one that could not
  /// answer.
  Future<void> loadRegistrationInfo() async {
    if (!state.isDomainValid) return;
    final domain = state.domain;

    try {
      final info = await getRegistrationInfo(
        domain: domain,
      ).timeout(_registrationInfoTimeout);
      // The answer describes the server that was asked, which is no longer the
      // one the flow points at.
      if (state.domain != domain) return;
      emit(state.copyWith(registrationInfo: info));
    } catch (e) {
      _log.warning("Could not read registration info: ${e.toString()}");
    }
  }

  Future<CheckInvitationCodeError?> submitInvitationCode() async {
    if (state.invitationCode == null) {
      return const CheckInvitationCodeError(code: .missing);
    }

    emit(state.copyWith(isCheckingInvitationCode: true));

    try {
      final isValid = await checkInvitationCode(
        domain: state.domain,
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
    // A challenge the server did not ask for is not sent: it would be spent
    // for nothing.
    RegistrationChallenge? challenge;
    if (state.invitationCodeRequired) {
      final code = state.invitationCode;
      if (code == null) {
        return const SignUpError(code: .challengeRequired);
      }
      challenge = RegistrationChallenge.invitationCode(code);
    }

    emit(state.copyWith(isSigningUp: true));

    try {
      _log.info("Registering user...");
      await _coreClient.createUser(
        state.domain,
        state.displayName,
        state.avatar?.data,
        challenge,
      );
    } on CreateUserError catch (e) {
      _log.severe("Error when registering user: ${e.toString()}");
      emit(state.copyWith(isSigningUp: false));
      return switch (e) {
        CreateUserError_ChallengeRequired(:final accepted) => _requireChallenge(
          accepted,
        ),
        CreateUserError_ChallengeRejected() => const SignUpError(
          code: .challengeRejected,
        ),
        CreateUserError_Other(:final message) => SignUpError(
          code: .internal,
          message: message,
        ),
      };
    } catch (e) {
      _log.severe("Error when registering user: ${e.toString()}");
      emit(state.copyWith(isSigningUp: false));
      return SignUpError(code: .internal, message: e.toString());
    }

    emit(state.copyWith(isSigningUp: false));

    return null;
  }

  /// Records that the server started asking for a challenge while the form was
  /// being filled in.
  ///
  /// The rejection carries the kinds the server accepts, which is fresher than
  /// asking it again would be.
  SignUpError _requireChallenge(List<ChallengeKind> accepted) {
    emit(
      state.copyWith(
        registrationInfo: RegistrationInfo(
          challengeRequired: true,
          acceptedChallenges: accepted,
        ),
      ),
    );
    return const SignUpError(code: .challengeRequired);
  }
}

final class SignUpError {
  const SignUpError({required this.code, this.message});
  final SignUpErrorCode code;
  final String? message;
}

enum SignUpErrorCode { challengeRequired, challengeRejected, internal }

final class CheckInvitationCodeError {
  const CheckInvitationCodeError({required this.code, this.message});
  final InvitationCodeError code;
  final String? message;
}

enum InvitationCodeError { missing, invalid, internal }
