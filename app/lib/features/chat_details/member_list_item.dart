// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/api/types.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/avatar.dart';
import 'package:flutter/material.dart';

class MemberListItem extends StatelessWidget {
  const MemberListItem({
    super.key,
    required this.profile,
    this.trailing,
    this.onTap,
    this.enabled = true,
    this.displayNameOverride,
  });

  final UiUserProfile profile;
  final Widget? trailing;
  final VoidCallback? onTap;
  final bool enabled;
  final String? displayNameOverride;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final displayName = displayNameOverride ?? profile.displayName;

    return ListTile(
      contentPadding: EdgeInsets.zero,
      minVerticalPadding: S.s12,
      enabled: enabled,
      onTap: onTap,
      leading: UserAvatar(profile: profile, size: S.s32),
      title: Text(
        displayName,
        style: typeScale.body.regular.style(color: palette.text.primary),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: trailing,
      hoverColor: palette.backgroundBase.secondary.withValues(alpha: 0.3),
    );
  }
}
