// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/nux/nux_pill.dart';
import 'package:air/ds/patterns/nux/nux_scaffold.dart';
import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:air/features/developer/developer_unlock.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/language_picker_menu.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/platform/notification_permissions.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter_svg/svg.dart';
import 'package:url_launcher/url_launcher.dart';

class IntroScreen extends HookWidget {
  const IntroScreen({super.key});

  /// The mark takes more room where there is more of it to take.
  static const double _logoWidthPhone = 104;
  static const double _logoWidthDesktop = S.s128;

  static final Uri _termsOfUseUri = Uri.parse('https://air.ms/terms');

  @override
  Widget build(BuildContext context) {
    final showOnboarding = context.select(
      (LoadableUserCubit cubit) => switch (cubit.state) {
        UnloadedUser() || UnloadingUser() => true,
        LoadingUser() || LoadedUser() => false,
      },
    );

    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final tokens = NuxScaffoldTokens.of(context);

    final serverFieldVisible = useState(false);
    final openingSignUp = useState(false);

    final bool experimentalFeatures = context.select(
      (UserSettingsCubit cubit) => cubit.state.experimentalFeaturesActive,
    );

    final onLogoTap = useDeveloperUnlock();

    openLinking() async {
      await requestNotificationPermission();
      if (!context.mounted) return;
      context.read<NavigationCubit>().openLinking();
    }

    openSignUp() async {
      if (openingSignUp.value) return;
      openingSignUp.value = true;
      try {
        await requestNotificationPermission();
        if (!context.mounted) return;
        // Asked here rather than in the flow, so the flow opens on the step the
        // server actually wants and never has to drop one the user is looking
        // at.
        await context.read<RegistrationCubit>().loadRegistrationInfo();
        if (!context.mounted) return;
        context.read<NavigationCubit>().openSignUp();
      } finally {
        // The flag goes with the screen, so one that left the tree has nothing
        // left to reset.
        if (context.mounted) openingSignUp.value = false;
      }
    }

    // A window gives the picker a top row inside the safe zone. A phone floats
    // it over the corner, leaving the mark centered in the full height.
    final isPhone = DeviceType.isPhone;
    const picker = _LanguagePicker();

    return NuxScaffold(
      tokens: tokens,
      top: isPhone ? null : picker,
      overlay: isPhone ? picker : null,
      body: SizedBox(
        width: isPhone ? _logoWidthPhone : _logoWidthDesktop,
        child: GestureDetector(
          // The mark is the only way into the developer surface before login,
          // and the glyph leaves gaps a tap would fall through.
          behavior: .opaque,
          onTap: onLogoTap,
          child: SvgPicture.asset(
            'assets/images/logo.svg',
            colorFilter: ColorFilter.mode(palette.text.primary, .srcIn),
          ),
        ),
      ),
      footer: showOnboarding
          ? Column(
              mainAxisSize: .min,
              crossAxisAlignment: .stretch,
              children: [
                _TermsOfUseText(loc: loc),
                const SizedBox(height: S.s16),
                if (serverFieldVisible.value) ...[
                  Text(
                    loc.introScreen_serverLabel,
                    style: Theme.of(context).textTheme.bodyMedium,
                  ),
                  const SizedBox(height: S.s16),
                  _ServerTextField(onFieldSubmitted: openLinking),
                  const SizedBox(height: S.s16),
                ],
                if (experimentalFeatures) ...[
                  Button(
                    type: .secondary,
                    label: loc.introScreen_linkExisting,
                    onPressed: openLinking,
                    onLongPress: () => serverFieldVisible.value = true,
                  ),
                  const SizedBox(height: S.s8),
                ],
                Button(
                  type: .primary,
                  label: loc.introScreen_signUp,
                  state: openingSignUp.value ? .pending : .active,
                  onPressed: openSignUp,
                ),
              ],
            )
          : null,
    );
  }
}

/// The language picker's trigger: the frame's pill in a top row, a glyph in a
/// disc where it floats over the screen.
class _LanguagePicker extends StatelessWidget {
  const _LanguagePicker();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return LanguagePickerMenu(
      onLocaleSelected: (locale) async {
        context.read<AppLocaleCubit>().setLocale(locale);
      },
      childBuilder: (context, option, onTap) {
        if (!DeviceType.isPhone) {
          return NuxPill(
            icon: AppIconType.globe,
            label: option.label,
            onTap: onTap,
          );
        }

        // The row carries no fill of its own, so the wash sits directly on the
        // screen behind it and the pill radius wraps the whole trigger.
        return StateLayer(
          borderRadius: CornerRadius.full,
          surface: NuxScaffoldTokens.surface(context),
          onTap: onTap,
          child: Row(
            mainAxisSize: .min,
            children: [
              Container(
                width: 36,
                height: 36,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: palette.backgroundBase.tertiary,
                  shape: .circle,
                ),
                child: AppIcon.globe(color: palette.text.secondary, size: 18),
              ),
              const SizedBox(width: S.s12),
              Text(
                option.label,
                style: typeScale.body.regular.style(
                  color: palette.text.primary,
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _TermsOfUseText extends StatelessWidget {
  const _TermsOfUseText({required this.loc});

  final AppLocalizations loc;

  @override
  Widget build(BuildContext context) {
    final baseTextStyle = typeScale.body.xs.style(
      color: SemanticPalette.of(context).text.tertiary,
    );

    final linkText = loc.introScreen_termsLinkText;
    final agreement = loc.introScreen_termsText(linkText);
    final linkStart = agreement.indexOf(linkText);

    if (linkStart == -1) {
      return Text(agreement, style: baseTextStyle, textAlign: .center);
    }

    final beforeLink = agreement.substring(0, linkStart);
    final afterLink = agreement.substring(linkStart + linkText.length);

    final linkStyle = baseTextStyle.copyWith(
      color: SemanticPalette.of(context).function.link,
    );

    return Text.rich(
      TextSpan(
        style: baseTextStyle,
        children: [
          TextSpan(text: beforeLink),
          TextSpan(
            text: linkText,
            style: linkStyle,
            recognizer: TapGestureRecognizer()
              ..onTap = () {
                launchUrl(
                  IntroScreen._termsOfUseUri,
                  mode: LaunchMode.externalApplication,
                );
              },
          ),
          TextSpan(text: afterLink),
        ],
      ),
      textAlign: .center,
    );
  }
}

class _ServerTextField extends HookWidget {
  const _ServerTextField({required this.onFieldSubmitted});

  final VoidCallback onFieldSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final focusNode = useFocusNode();
    final controller = useTextEditingController(
      text: context.read<RegistrationCubit>().state.domain,
    );

    return AppTextInput(
      tokens: AppTextInputTokens.current,
      controller: controller,
      focusNode: focusNode,
      hintText: loc.introScreen_serverHint,
      onChanged: (String value) {
        context.read<RegistrationCubit>().setDomain(value);
      },
      onSubmitted: (_) {
        focusNode.requestFocus();
        if (context.read<RegistrationCubit>().state.isDomainValid) {
          onFieldSubmitted();
        }
      },
    );
  }
}
