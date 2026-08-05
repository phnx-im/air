// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/user/avatar.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class SafetyCodeView extends StatelessWidget {
  const SafetyCodeView({super.key, required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const SizedBox(height: S.s12),

          UserAvatar(profile: profile, size: 192),

          const SizedBox(height: S.s16),

          Text(
            profile.displayName,
            style: typeScale.header.xl.style(weight: Weight.emphasized),
          ),

          const SizedBox(height: S.s24),

          _SafetyCode(userId: profile.userId),

          const SizedBox(height: S.s24),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s16),
            child: Text(
              style: typeScale.body.s.style(color: palette.text.tertiary),
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
    final palette = SemanticPalette.of(context);

    final codeStyle = typeScale.body.regular
        .style(
          color: safetyCode.hasData
              ? palette.text.primary
              : palette.text.tertiary,
        )
        .withSystemMonospace()
        .copyWith(height: 1.5);

    return InkWell(
      onTap: safetyCode.hasData
          ? () {
              // copy to clipboard
              Clipboard.setData(
                ClipboardData(text: safetyCode.data!.textRepresentation),
              );
              showSnackBarStandalone(
                (loc) => SnackBar(
                  content: Text(loc.safetyCodeScreen_copiedToClipboard),
                ),
              );
            }
          : null,
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(CornerRadius.px12),
          color: palette.backgroundBase.secondary,
        ),
        padding: const EdgeInsets.symmetric(vertical: S.s16, horizontal: S.s24),
        child: Column(
          children: [
            Text(p1, style: codeStyle),
            Text(p2, style: codeStyle),
            Text(p3, style: codeStyle),
            const SizedBox(height: S.s24),
            Row(
              mainAxisSize: .min,
              mainAxisAlignment: .center,
              children: [
                AppIcon.copy(color: palette.text.tertiary, size: 16),
                const SizedBox(width: S.s8),
                Text(
                  loc.safetyCodeScreen_tapToCopy,
                  style: typeScale.body.s.style(color: palette.text.tertiary),
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
