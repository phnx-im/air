// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/api/types.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/widgets/widgets.dart';
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
    final colors = CustomColorScheme.of(context);
    final displayName = displayNameOverride ?? profile.displayName;

    return ListTile(
      contentPadding: EdgeInsets.zero,
      minVerticalPadding: Spacings.xs,
      enabled: enabled,
      onTap: onTap,
      leading: UserAvatar(userId: profile.userId, size: Spacings.l),
      title: Text(
        displayName,
        style: TextStyle(
          fontSize: LabelFontSize.base.size,
          color: colors.text.primary,
        ),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: trailing,
      hoverColor: colors.backgroundBase.secondary.withValues(alpha: 0.3),
    );
  }
}
