// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/platform/method_channel.dart';
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

/// How long the challenge gets to arrive before the flow goes with its
/// fallback.
const _challengeTimeout = Duration(seconds: 8);

/// How much of a session's lifetime is left unused. The expiry is anchored to
/// the clock at receipt, which runs ahead of the server's by one round trip,
/// and the registration itself takes time to land.
const _admissionExpiryMargin = Duration(seconds: 30);

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

    /// The admission session this device holds, and when it stops being
    /// spendable.
    AdmissionSession? admissionSession,
    DateTime? admissionExpiresAt,
  }) = _RegistrationState;

  /// Whether the flow has to answer a challenge at all. True before the server
  /// has answered, since a gated server is the case to be ready for.
  bool get challengeRequired => registrationInfo?.challengeRequired ?? true;

  bool _serverTakes(ChallengeKind kind) =>
      registrationInfo?.acceptedChallenges.contains(kind) ?? false;

  bool get serverTakesAdmissionSession =>
      _serverTakes(ChallengeKind.admissionSession);

  /// Whether a session is in hand and still spendable.
  bool get hasAdmissionSession {
    final expiresAt = admissionExpiresAt;
    return admissionSession != null &&
        expiresAt != null &&
        expiresAt
            .subtract(_admissionExpiryMargin)
            .isAfter(DateTime.now().toUtc());
  }

  /// Whether the flow collects an invitation code, which is what it falls back
  /// to without a session in hand.
  bool get invitationCodeRequired {
    if (!challengeRequired || hasAdmissionSession) return false;
    if (registrationInfo == null) return true;
    return _serverTakes(ChallengeKind.invitationCode);
  }

  /// Whether the server asks for something this flow has no way to supply.
  bool get challengeUnsupported {
    if (!challengeRequired || registrationInfo == null) return false;
    return !hasAdmissionSession && !_serverTakes(ChallengeKind.invitationCode);
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

  /// Acquires an admission session, where the server offers one.
  ///
  /// Every failure is silent and leaves the flow asking for an invitation code.
  Future<void> acquireAdmissionSession() async {
    if (!state.serverTakesAdmissionSession || state.hasAdmissionSession) return;

    final pushToken = await getPushToken();
    if (pushToken == null) return;

    try {
      final session = await createAdmissionSession(
        domain: state.domain,
        pushToken: pushToken,
      );
      final challenge = await awaitAdmissionChallenge(
        session.sessionId,
        _challengeTimeout,
      );
      if (challenge == null) return;
      emit(
        state.copyWith(
          admissionSession: AdmissionSession(
            sessionId: session.sessionId,
            challenge: challenge,
          ),
          admissionExpiresAt: DateTime.now().toUtc().add(session.lifetime),
        ),
      );
    } catch (e) {
      _log.warning("Could not acquire an admission session: ${e.toString()}");
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
    emit(state.copyWith(isSigningUp: true));
    try {
      return await _createUser();
    } finally {
      emit(state.copyWith(isSigningUp: false));
    }
  }

  Future<SignUpError?> _createUser() async {
    // Sessions live minutes and filling the form in takes longer, so an expired
    // one is replaced here.
    if (state.challengeRequired &&
        !state.hasAdmissionSession &&
        state.invitationCode == null) {
      await acquireAdmissionSession();
    }

    // A challenge the server did not ask for would be spent for nothing.
    RegistrationChallenge? challenge;
    if (state.challengeRequired) {
      final session = state.admissionSession;
      final code = state.invitationCode;
      if (state.hasAdmissionSession && session != null) {
        challenge = RegistrationChallenge.admissionSession(session);
      } else if (state.invitationCodeRequired && code != null) {
        challenge = RegistrationChallenge.invitationCode(code);
      } else {
        return const SignUpError(code: .challengeRequired);
      }
    }
    final answeredWithSession =
        challenge is RegistrationChallenge_AdmissionSession;

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
      return switch (e) {
        CreateUserError_ChallengeRequired(:final accepted) => _requireChallenge(
          accepted,
        ),
        CreateUserError_ChallengeRejected() => _challengeRejected(
          answeredWithSession,
        ),
        CreateUserError_Other(:final message) => SignUpError(
          code: .internal,
          message: message,
        ),
      };
    } catch (e) {
      _log.severe("Error when registering user: ${e.toString()}");
      return SignUpError(code: .internal, message: e.toString());
    }

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

  /// A turned-down session is gone for good, so the flow drops it and asks for
  /// a challenge again.
  SignUpError _challengeRejected(bool answeredWithSession) {
    if (!answeredWithSession) {
      return const SignUpError(code: .challengeRejected);
    }
    emit(state.copyWith(admissionSession: null, admissionExpiresAt: null));
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
