// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
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
    return Scaffold(
      appBar: AppBar(title: Text(loc.safetyCodeScreen_title)),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(Spacings.m),
        child: SafetyCodeView(profile: profile),
      ),
    );
  }
}

class SafetyCodeView extends StatelessWidget {
  const SafetyCodeView({super.key, required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const SizedBox(height: Spacings.s),

          UserAvatar(size: 192, userId: profile.userId, profile: profile),

          const SizedBox(height: Spacings.s),

          Text(
            profile.displayName,
            style: TextStyle(
              fontSize: HeaderFontSize.h1.size,
              fontWeight: FontWeight.bold,
            ),
          ),

          const SizedBox(height: Spacings.s),

          _SafetyCodeButton(userId: profile.userId),

          const SizedBox(height: Spacings.s),

          Text(
            style: TextStyle(
              fontSize: BodyFontSize.small1.size,
              color: CustomColorScheme.of(context).text.tertiary,
            ),
            loc.safetyCodeScreen_safetyCodeExplanation(profile.displayName),
          ),
        ],
      ),
    );
  }
}

class _SafetyCodeButton extends HookWidget {
  const _SafetyCodeButton({required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final safetyCodeFut = useMemoized(
      () => context.read<UserCubit>().safetyCodes(userId),
      [userId],
    );
    final safetyCode = useFuture(safetyCodeFut);

    final loc = AppLocalizations.of(context);

    return InkWell(
      onTap: safetyCode.hasData
          ? () {
              // copy to clipboard
              Clipboard.setData(
                ClipboardData(text: formatStringArray12(safetyCode.data!)),
              );
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text(loc.safetyCodeScreen_copiedToClipboard)),
              );
            }
          : null,
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(12),
          color: CustomColorScheme.of(context).backgroundBase.secondary,
        ),
        padding: const EdgeInsets.symmetric(
          vertical: Spacings.m,
          horizontal: Spacings.xxs,
        ),
        child: Column(
          children: [
            Text(
              safetyCode.data != null
                  ? formatStringArray12(safetyCode.data!)
                  : "Loading...",
              style: TextStyle(fontSize: LabelFontSize.base.size),
            ),
            const SizedBox(height: Spacings.s),
            Row(
              mainAxisAlignment: .center,
              children: [
                iconoir.Copy(
                  color: CustomColorScheme.of(context).text.tertiary,
                  width: 12,
                  height: 12,
                ),
                const SizedBox(width: Spacings.xxs),
                Text(
                  loc.safetyCodeScreen_tapToCopy,
                  style: TextStyle(
                    fontSize: BodyFontSize.small1.size,
                    color: CustomColorScheme.of(context).text.tertiary,
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

String formatStringArray12(StringArray12 arr) {
  final parts = <String>[];

  for (var i = 0; i < arr.length; i += 4) {
    parts.add('${arr[i]} ${arr[i + 1]}\n${arr[i + 2]} ${arr[i + 3]}');
  }

  // Join groups with a *blank line* between them
  return parts.join('\n\n');
}
