// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/mention_pill/mention_pill.dart';
import 'package:air/ds/components/mention_pill/mention_pill_tokens.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';

/// [MentionPill] for a chat member.
///
/// Shows the receiver-local display name of the mentioned user.
class UserMentionPill extends StatelessWidget {
  const UserMentionPill({
    super.key,
    required this.userId,
    this.isSelfMention = false,
    this.tappable = false,
  });

  final UiUserId userId;
  final bool isSelfMention;

  /// Whether tapping the pill opens the member details.
  final bool tappable;

  @override
  Widget build(BuildContext context) {
    final displayName = context.select(
      (UsersCubit cubit) => cubit.state.displayName(userId: userId),
    );

    return MentionPill(
      name: displayName,
      variant: isSelfMention ? MentionPillVariant.me : MentionPillVariant.other,
      onTap: tappable ? () => _openMemberDetails(context) : null,
    );
  }

  void _openMemberDetails(BuildContext context) {
    try {
      context.read<NavigationCubit>().openMemberDetails(userId);
    } on ProviderNotFoundException {
      // No navigation available, for example in tests. Ignore the tap.
    }
  }
}
