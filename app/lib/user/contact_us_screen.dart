// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:url_launcher/url_launcher.dart' as url_launcher;

final _log = Logger('ContactUsScreen');

class ContactUsScreen extends StatelessWidget {
  const ContactUsScreen({
    super.key,
    this.initialSubject,
    this.initialBody,
    this.launcher,
  });

  final String? initialSubject;
  final String? initialBody;
  final UrlLauncher? launcher;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(
          loc.contactUsScreen_title,
          style: TextStyle(
            fontSize: LabelFontSize.base.size,
            fontWeight: FontWeight.bold,
          ),
        ),
        leading: AppBarBackButton(
          backgroundColor: colors.backgroundElevated.primary,
        ),
        actions: null,
        backgroundColor: Colors.transparent,
        toolbarHeight: isPointer() ? 100 : null,
        centerTitle: true,
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        minimum: const EdgeInsets.only(bottom: Spacings.l + Spacings.xxs),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
          child: Align(
            alignment: Alignment.topCenter,
            child: Container(
              constraints:
                  isPointer() ? const BoxConstraints(maxWidth: 800) : null,
              child: Theme(
                data: theme.copyWith(
                  inputDecorationTheme: theme.inputDecorationTheme.copyWith(
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: Spacings.xxs,
                      vertical: Spacings.xxs,
                    ),
                    isDense: true,
                    border: _outlineInputBorder,
                    enabledBorder: _outlineInputBorder,
                    focusedBorder: _outlineInputBorder,
                    filled: true,
                    fillColor: colors.backgroundBase.tertiary,
                  ),
                ),
                child: _EmailForm(
                  initialBody: initialBody,
                  initialSubject: initialSubject,
                  launcher: launcher ?? _UrlLauncher(),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _EmailForm extends HookWidget {
  _EmailForm({this.initialBody, this.initialSubject, required this.launcher});

  final _formKey = GlobalKey<FormState>();

  final String? initialBody;
  final String? initialSubject;
  final UrlLauncher launcher;

  @override
  Widget build(BuildContext context) {
    final body = useState(initialBody ?? "");
    final selectedSubject = useState<String?>(initialSubject);

    final loc = AppLocalizations.of(context);

    final List<String> subjects = [
      loc.contactUsScreen_subject_somethingNotWorking,
      loc.contactUsScreen_subject_iHaveAQuestion,
      loc.contactUsScreen_subject_requestFeature,
      loc.contactUsScreen_subject_other,
    ];

    assert(initialSubject == null || subjects.contains(initialSubject));

    return Form(
      key: _formKey,
      child: SingleChildScrollView(
        child: Column(
          children: [
            // Spacing for the label of Subject Dropdown field (when selected)
            const SizedBox(height: Spacings.xxs),

            // Subject Dropdown
            DropdownButtonFormField<String>(
              initialValue: initialSubject,
              decoration: InputDecoration(
                labelText: loc.contactUsScreen_subject,
              ),
              items:
                  subjects
                      .map(
                        (subject) => DropdownMenuItem(
                          value: subject,
                          child: Text(subject),
                        ),
                      )
                      .toList(),
              onChanged: (value) => selectedSubject.value = value,
              validator: (value) => _validateSubject(value, loc),
            ),
            const SizedBox(height: Spacings.s),

            // Email Body
            TextFormField(
              initialValue: initialBody,
              maxLines: 6,
              decoration: InputDecoration(
                labelText: loc.contactUsScreen_body,
                alignLabelWithHint: true,
              ),
              onSaved: (value) => body.value = value ?? "",
              validator: (value) => _validateBody(value, loc),
            ),
            const SizedBox(height: 24),

            // Submit Button
            OutlinedButton(
              style: const ButtonStyle(
                shape: WidgetStatePropertyAll(
                  RoundedRectangleBorder(
                    borderRadius: BorderRadius.all(
                      Radius.circular(Spacings.xs),
                    ),
                  ),
                ),
              ),
              onPressed: () {
                final formState = _formKey.currentState;
                if (formState != null && formState.validate()) {
                  formState.save();
                  _launchEmail(context, selectedSubject.value, body.value);
                }
              },
              child: Text(
                loc.contactUsScreen_composeEmail,
                style: TextStyle(fontSize: LabelFontSize.base.size),
              ),
            ),
          ],
        ),
      ),
    );
  }

  String? _validateSubject(String? value, AppLocalizations loc) =>
      value == null || value.isEmpty ? loc.contactUsScreen_subject_empty : null;

  String? _validateBody(String? value, AppLocalizations loc) =>
      value == null || value.isEmpty
          ? loc.contactUsScreen_body_empty
          : value.length < 11
          ? loc.contactUsScreen_body_tooShort
          : null;

  void _launchEmail(BuildContext context, String? subject, String body) async {
    final Uri emailUri = Uri.parse(
      'mailto:help@air.ms?subject=$subject&body=$body',
    );

    final loc = AppLocalizations.of(context);

    try {
      await launcher.launchUrl(emailUri);
    } catch (e) {
      _log.severe("Failed to launch email: $e");
      if (context.mounted) {
        showErrorBanner(context, loc.contactUsScreen_errorLaunchingEmail);
      }
    }
  }
}

abstract class UrlLauncher {
  Future<void> launchUrl(Uri url);
}

class _UrlLauncher implements UrlLauncher {
  @override
  Future<void> launchUrl(Uri url) => url_launcher.launchUrl(url);
}

const _outlineInputBorder = OutlineInputBorder(
  borderRadius: BorderRadius.all(Radius.circular(Spacings.s)),
  borderSide: BorderSide(width: 0, style: BorderStyle.none),
);
