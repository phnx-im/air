// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/icons/icons.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/registration/registration_cubit.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/l10n/language_picker_menu.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/util/notification_permissions.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter_svg/svg.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:air/ds/components/desktop/width_constraints.dart';

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

    final colors = CustomColorScheme.of(context);

    final serverFieldVisible = useState(false);

    final textFormConstraints = BoxConstraints.tight(
      isSmallScreen(context)
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
      backgroundColor: colors.backgroundBase.secondary,
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
                      colors.text.primary,
                      BlendMode.srcIn,
                    ),
                  ),
                ),
              ),
            ),
            const Align(
              alignment: Alignment.topLeft,
              child: Padding(
                padding: EdgeInsets.only(left: Spacing.px24, top: Spacing.px24),
                child: _LanguagePicker(),
              ),
            ),
            if (!isUserLoading)
              Align(
                alignment: Alignment.bottomCenter,
                child: ConstrainedWidth(
                  width: isSmallScreen(context) ? double.infinity : 320,
                  child: Padding(
                    padding: isSmallScreen(context)
                        ? const EdgeInsets.symmetric(horizontal: Spacing.px16)
                        : EdgeInsets.zero,
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        _TermsOfUseText(loc: loc),
                        const SizedBox(height: Spacing.px16),
                        if (serverFieldVisible.value) ...[
                          Text(
                            loc.introScreen_serverLabel,
                            style: Theme.of(context).textTheme.bodyMedium,
                            textAlign: TextAlign.left,
                          ),
                          const SizedBox(height: Spacing.px16),

                          ConstrainedBox(
                            constraints: textFormConstraints,
                            child: _ServerTextField(
                              onFieldSubmitted: openLinking,
                            ),
                          ),
                        ],
                        if (isDeveloper) ...[
                          AppButton(
                            type: .secondary,
                            label: loc.introScreen_linkExisting,
                            onPressed: openLinking,
                            onLongPress: () => serverFieldVisible.value = true,
                          ),
                          const SizedBox(height: Spacing.px8),
                        ],
                        AppButton(
                          type: .primary,
                          label: loc.introScreen_signUp,
                          onPressed: () async {
                            await requestNotificationPermission();
                            if (!context.mounted) return;
                            context.read<NavigationCubit>().openSignUp();
                          },
                        ),
                        const SizedBox(height: Spacing.px16),
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
    final colors = CustomColorScheme.of(context);

    return LanguagePickerMenu(
      onLocaleSelected: (locale) async {
        context.read<AppLocaleCubit>().setLocale(locale);
      },
      childBuilder: (context, option, onTap) {
        return TextButton(
          style: TextButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: Size.zero,
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          ),
          onPressed: onTap,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 36,
                height: 36,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: colors.backgroundBase.tertiary,
                  shape: BoxShape.circle,
                ),
                child: AppIcon.globe(color: colors.text.secondary, size: 18),
              ),
              const SizedBox(width: Spacing.px12),
              Text(
                option.label,
                style: TextStyle(
                  fontSize: LabelFontSize.base.size,
                  color: colors.text.primary,
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
    final baseTextStyle = TextStyle(
      fontSize: LabelFontSize.small2.size,
      color: CustomColorScheme.of(context).text.tertiary,
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
      color: CustomColorScheme.of(context).function.link,
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
    final colors = CustomColorScheme.of(context);
    final focusNode = useFocusNode();

    return TextFormField(
      decoration: InputDecoration(
        hintText: loc.introScreen_serverHint,
        fillColor: colors.backgroundBase.tertiary,
      ),
      initialValue: context.read<RegistrationCubit>().state.domain,
      focusNode: focusNode,
      onChanged: (String value) {
        context.read<RegistrationCubit>().setDomain(value);
      },
      onFieldSubmitted: (_) {
        focusNode.requestFocus();
        if (context.read<RegistrationCubit>().state.isDomainValid) {
          onFieldSubmitted();
        }
      },
      validator: (value) =>
          context.read<RegistrationCubit>().state.isDomainValid
          ? null
          : loc.introScreen_error_invalidDomain,
    );
  }
}
