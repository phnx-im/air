// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/status_label/status_label.dart';
import 'package:air/ds/components/status_label/status_label_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';

/// How a verification reads in a status line.
extension ContactVerificationStatus on UiContactVerification {
  String label(AppLocalizations loc) => switch (this) {
    UiContactVerification.verified => loc.contactVerification_verified,
    UiContactVerification.unverified => loc.contactVerification_unverified,
  };

  AppIconType get icon => switch (this) {
    UiContactVerification.verified => AppIconType.shield,
    UiContactVerification.unverified => AppIconType.user,
  };
}

/// The verification of a user, as the status line under their name on a
/// profile.
class ContactVerificationLabel extends StatelessWidget {
  const ContactVerificationLabel({super.key, required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final verification = context.select(
      (UsersCubit cubit) => cubit.state.verification(userId: userId),
    );
    return StatusLabel(
      tokens: StatusLabelTokens.profile,
      label: verification.label(AppLocalizations.of(context)),
      icon: verification.icon,
    );
  }
}
