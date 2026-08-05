// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
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

import 'package:air/ds/components/constrained_width/constrained_width.dart';

class IntroScreen extends HookWidget {
  const IntroScreen({super.key});

  static const double _logoWidth = 104;
  static final Uri _termsOfUseUri = Uri.parse('https://air.ms/terms');

  @override
  Widget build(BuildContext context) {
    final isUserLoading = context.select((LoadableUserCubit cubit) {
      return cubit.state is LoadingUser;
    });

    final loc = AppLocalizations.of(context);

    final palette = SemanticPalette.of(context);

    final serverFieldVisible = useState(false);

    final textFormConstraints = BoxConstraints.tight(
      context.breakpoint.isSmall
          ? const Size(double.infinity, 120)
          : const Size(300, 120),
    );

    final bool isDeveloper = context.select(
      (UserSettingsCubit cubit) => cubit.state.isDeveloper,
    );

    openLinking() async {
      await requestNotificationPermission();
      if (!context.mounted) return;
      context.read<NavigationCubit>().openLinking();
    }

    return Scaffold(
      backgroundColor: palette.backgroundBase.secondary,
      body: SafeArea(
        child: Stack(
          children: [
            Align(
              alignment: Alignment.center,
              child: SizedBox(
                width: _logoWidth,
                child: GestureDetector(
                  onLongPress: () {
                    context.read<NavigationCubit>().openDeveloperSettings();
                  },
                  child: SvgPicture.asset(
                    'assets/images/logo.svg',
                    colorFilter: ColorFilter.mode(
                      palette.text.primary,
                      BlendMode.srcIn,
                    ),
                  ),
                ),
              ),
            ),
            const Align(
              alignment: Alignment.topLeft,
              child: Padding(
                padding: EdgeInsets.only(left: S.s24, top: S.s24),
                child: _LanguagePicker(),
              ),
            ),
            if (!isUserLoading)
              Align(
                alignment: Alignment.bottomCenter,
                child: ConstrainedWidth(
                  width: context.breakpoint.isSmall ? double.infinity : 320,
                  child: Padding(
                    padding: context.breakpoint.isSmall
                        ? const EdgeInsets.symmetric(horizontal: S.s16)
                        : EdgeInsets.zero,
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        _TermsOfUseText(loc: loc),
                        const SizedBox(height: S.s16),
                        if (serverFieldVisible.value) ...[
                          Text(
                            loc.introScreen_serverLabel,
                            style: Theme.of(context).textTheme.bodyMedium,
                            textAlign: TextAlign.left,
                          ),
                          const SizedBox(height: S.s16),

                          ConstrainedBox(
                            constraints: textFormConstraints,
                            child: _ServerTextField(
                              onFieldSubmitted: openLinking,
                            ),
                          ),
                        ],
                        if (isDeveloper) ...[
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
                          onPressed: () async {
                            await requestNotificationPermission();
                            if (!context.mounted) return;
                            context.read<NavigationCubit>().openSignUp();
                          },
                        ),
                        const SizedBox(height: S.s16),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

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
        // The row carries no fill of its own, so the wash sits directly on the
        // screen behind it and the pill radius wraps the whole trigger.
        return StateLayer(
          borderRadius: CornerRadius.full,
          surface: palette.backgroundBase.secondary,
          hover: !DeviceType.isPhone,
          pressScale: DeviceType.isPhone,
          onTap: onTap,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 36,
                height: 36,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: palette.backgroundBase.tertiary,
                  shape: BoxShape.circle,
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
      return Text(agreement, style: baseTextStyle, textAlign: TextAlign.center);
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
      textAlign: TextAlign.center,
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
