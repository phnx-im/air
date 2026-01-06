// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/ui/typography/monospace.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class SafetyCodeScreen extends HookWidget {
  const SafetyCodeScreen({super.key, required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: userId),
    );
    return AppScaffold(
      title: loc.safetyCodeScreen_title,
      child: SafetyCodeView(profile: profile),
    );
  }
}

class SafetyCodeView extends StatelessWidget {
  const SafetyCodeView({super.key, required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const SizedBox(height: Spacings.xs),

          UserAvatar(size: 192, userId: profile.userId, profile: profile),

          const SizedBox(height: Spacings.s),

          Text(
            profile.displayName,
            style: TextStyle(
              fontSize: HeaderFontSize.h1.size,
              fontWeight: FontWeight.bold,
            ),
          ),

          const SizedBox(height: Spacings.m),

          _SafetyCode(userId: profile.userId),

          const SizedBox(height: Spacings.m),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
            child: Text(
              style: TextStyle(
                fontSize: BodyFontSize.small1.size,
                color: colors.text.tertiary,
              ),
              loc.safetyCodeScreen_safetyCodeExplanation(profile.displayName),
            ),
          ),
        ],
      ),
    );
  }
}

class _SafetyCode extends HookWidget {
  const _SafetyCode({required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final Future<intArray12> safetyCodeFut = useMemoized(
      () => context.read<UserCubit>().safetyCodes(userId),
      [userId],
    );
    final safetyCode = useFuture(safetyCodeFut);
    final (p1, p2, p3) = safetyCode.data?.paragraphs ?? ('', '', '');

    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final codeStyle = TextStyle(
      fontSize: BodyFontSize.base.size,
      fontFamily: getSystemMonospaceFontFamily(),
      height: 1.5,
      color: safetyCode.hasData ? colors.text.primary : colors.text.tertiary,
    );

    return InkWell(
      onTap: safetyCode.hasData
          ? () {
              // copy to clipboard
              Clipboard.setData(
                ClipboardData(text: safetyCode.data!.textRepresentation),
              );
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text(loc.safetyCodeScreen_copiedToClipboard)),
              );
            }
          : null,
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(12),
          color: colors.backgroundBase.secondary,
        ),
        padding: const EdgeInsets.symmetric(
          vertical: Spacings.s,
          horizontal: Spacings.m,
        ),
        child: Column(
          children: [
            Text(p1, style: codeStyle),
            Text(p2, style: codeStyle),
            Text(p3, style: codeStyle),
            const SizedBox(height: Spacings.m),
            Row(
              mainAxisSize: .min,
              mainAxisAlignment: .center,
              children: [
                AppIcon.copy(color: colors.text.tertiary, size: 16),
                const SizedBox(width: Spacings.xxs),
                Text(
                  loc.safetyCodeScreen_tapToCopy,
                  style: TextStyle(
                    fontSize: BodyFontSize.small1.size,
                    color: colors.text.tertiary,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

extension on intArray12 {
  String get textRepresentation {
    final (p1, p2, p3) = paragraphs;
    return [p1, p2, p3].join('\n');
  }

  (String, String, String) get paragraphs {
    String sliceToString(List<int> slice) => slice
        .map((i) => i.toString().padLeft(5, '0'))
        .slices(2)
        .map((slice) => slice.join(' '))
        .join(' ');
    return (
      sliceToString(inner.sublist(0, 4)),
      sliceToString(inner.sublist(4, 8)),
      sliceToString(inner.sublist(8, 12)),
    );
  }
}
