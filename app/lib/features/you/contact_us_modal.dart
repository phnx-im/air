// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/checkbox/checkbox.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_guard.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:url_launcher/url_launcher.dart' as url_launcher;

final _log = Logger('ContactUsScreen');

/// Opens the contact form on the surface the device calls for.
Future<void> showContactUs(BuildContext context) => showAppModal<void>(
  context: context,
  builder: (_) => const ContactUsModal(),
);

/// The contact form: a subject, a body, and what to attach to them.
class ContactUsModal extends StatelessWidget {
  const ContactUsModal({
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

    return ModalScaffold(
      title: loc.contactUsScreen_title,
      onDismiss: () => Navigator.of(context).pop(),
      child: ModalBody(
        top: S.s8,
        child: _EmailForm(
          initialBody: initialBody,
          initialSubject: initialSubject,
          launcher: launcher ?? _UrlLauncher(),
        ),
      ),
    );
  }
}

class _EmailForm extends HookWidget {
  const _EmailForm({
    this.initialBody,
    this.initialSubject,
    required this.launcher,
  });

  final String? initialBody;
  final String? initialSubject;
  final UrlLauncher launcher;

  @override
  Widget build(BuildContext context) {
    final bodyController = useTextEditingController(text: initialBody);
    final bodyError = useState<String?>(null);
    final selectedSubject = useState<String?>(initialSubject);
    final subjectError = useState<String?>(null);
    final isUploadingLogs = useState(false);
    final debugLogsUrl = useState<String?>(null);

    Future<void> onToggleLogs() async {
      if (debugLogsUrl.value != null) {
        debugLogsUrl.value = null;
        return;
      }
      isUploadingLogs.value = true;
      try {
        debugLogsUrl.value = await context.read<UserCubit>().uploadLogs();
      } catch (e) {
        _log.severe("Failed to upload logs: $e", e);
        showErrorBannerStandalone(
          (loc) => loc.contactUsScreen_errorUploadingLogs,
        );
        debugLogsUrl.value = null;
      } finally {
        isUploadingLogs.value = false;
      }
    }

    final loc = AppLocalizations.of(context);

    final List<String> subjects = [
      loc.contactUsScreen_subject_somethingNotWorking,
      loc.contactUsScreen_subject_iHaveAQuestion,
      loc.contactUsScreen_subject_requestFeature,
      loc.contactUsScreen_subject_other,
    ];

    assert(initialSubject == null || subjects.contains(initialSubject));

    // The host scrolls this, so the form is a plain column.
    return ModalDismissGuard(
      hasUnsavedInput: () =>
          bodyController.text.trim().isNotEmpty ||
          selectedSubject.value != null,
      child: Column(
        children: [
          _SubjectField(
            subjects: subjects,
            selected: selectedSubject.value,
            errorText: subjectError.value,
            onSelected: (subject) {
              selectedSubject.value = subject;
              subjectError.value = null;
            },
          ),
          const SizedBox(height: S.s16),

          // Email Body
          AppTextInput(
            tokens: AppTextInputTokens.current,
            controller: bodyController,
            label: loc.contactUsScreen_body,
            errorText: bodyError.value,
            minLines: 6,
            maxLines: 6,
          ),
          const SizedBox(height: S.s8),

          // Include logs checkbox
          GestureDetector(
            behavior: .opaque,
            onTap: isUploadingLogs.value ? null : onToggleLogs,
            child: SizedBox(
              // The box paints at 20px, so the row carries the tap target.
              height: S.s48,
              child: Row(
                spacing: S.s12,
                children: [
                  if (isUploadingLogs.value)
                    const SizedBox(
                      width: S.s20,
                      height: S.s20,
                      child: CircularProgressIndicator(
                        strokeWidth: StrokeWidth.px2,
                      ),
                    )
                  else
                    AppCheckbox(
                      value: debugLogsUrl.value != null,
                      onChanged: (_) => onToggleLogs(),
                    ),
                  Text(loc.contactUsScreen_includeLogs),
                ],
              ),
            ),
          ),
          const SizedBox(height: S.s8),

          // Submit button. While the log upload runs, it is merely unavailable.
          Button(
            onPressed: () {
              final body = bodyController.text;
              bodyError.value = _validateBody(body, loc);
              subjectError.value = _validateSubject(selectedSubject.value, loc);
              if (subjectError.value == null && bodyError.value == null) {
                _launchEmail(
                  context,
                  selectedSubject.value,
                  body,
                  debugLogsUrl.value,
                );
              }
            },
            label: loc.contactUsScreen_composeEmail,
            type: ButtonType.secondary,
            state: isUploadingLogs.value
                ? ButtonState.disabled
                : ButtonState.active,
          ),
        ],
      ),
    );
  }

  String? _validateSubject(String? value, AppLocalizations loc) =>
      value == null || value.isEmpty ? loc.contactUsScreen_subject_empty : null;

  String? _validateBody(String value, AppLocalizations loc) => value.isEmpty
      ? loc.contactUsScreen_body_empty
      : value.length < 11
      ? loc.contactUsScreen_body_tooShort
      : null;

  void _launchEmail(
    BuildContext context,
    String? subject,
    String body,
    String? debugLogsUrl,
  ) async {
    if (debugLogsUrl != null) {
      final loc = AppLocalizations.of(context);
      body += "\n\n";
      body += loc.contactUsScreen_body_logsUrl(
        Uri.encodeComponent(debugLogsUrl),
      );
    }
    final Uri emailUri = Uri.parse(
      'mailto:help@air.ms?subject=$subject&body=$body',
    );
    try {
      await launcher.launchUrl(emailUri);
    } catch (e) {
      _log.severe("Failed to launch email: $e", e);
      showErrorBannerStandalone(
        (loc) => loc.contactUsScreen_errorLaunchingEmail,
      );
    }
  }
}

/// Picks one of [subjects] from a menu, wearing [AppTextInput]'s chrome.
class _SubjectField extends StatelessWidget {
  const _SubjectField({
    required this.subjects,
    required this.selected,
    required this.errorText,
    required this.onSelected,
  });

  final List<String> subjects;
  final String? selected;
  final String? errorText;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final errored = errorText != null;
    final value = selected;

    return Column(
      crossAxisAlignment: .stretch,
      mainAxisSize: .min,
      children: [
        Padding(
          padding: AppTextInputTokens.labelPadding,
          child: Text(
            loc.contactUsScreen_subject,
            style: typeScale.body.s.style(color: palette.text.quaternary),
          ),
        ),
        const SizedBox(height: AppTextInputTokens.labelGap),
        GestureDetector(
          behavior: .opaque,
          onTap: () => _open(context),
          child: Container(
            padding: AppTextInputTokens.current.fieldPadding,
            decoration: BoxDecoration(
              color: palette.fill.tertiary,
              borderRadius: BorderRadius.circular(AppTextInputTokens.radius),
              border: Border.all(
                color: errored ? palette.function.danger : Colors.transparent,
                width: AppTextInputTokens.borderWidth,
              ),
            ),
            child: Row(
              spacing: S.s8,
              children: [
                Expanded(
                  child: Text(
                    value ?? loc.contactUsScreen_subject_empty,
                    maxLines: 1,
                    overflow: .ellipsis,
                    style: typeScale.body.regular
                        .style(
                          color: value == null
                              ? palette.text.tertiary
                              : palette.text.primary,
                        )
                        .copyWith(height: 1.0),
                  ),
                ),
                AppIcon.chevronDown(size: 16, color: palette.text.tertiary),
              ],
            ),
          ),
        ),
        if (errorText != null) ...[
          const SizedBox(height: AppTextInputTokens.helperGap),
          Padding(
            padding: AppTextInputTokens.helperPadding,
            child: Text(
              errorText!,
              style: typeScale.body.s.style(color: palette.function.danger),
            ),
          ),
        ],
      ],
    );
  }

  void _open(BuildContext context) {
    final render = context.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;

    unawaited(
      showOverlayMenu(
        context: context,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        items: [
          for (final subject in subjects)
            MenuItem(
              label: subject,
              selected: subject == selected,
              onPressed: () => onSelected(subject),
            ),
        ],
      ),
    );
  }
}

abstract class UrlLauncher {
  Future<void> launchUrl(Uri url);
}

class _UrlLauncher implements UrlLauncher {
  @override
  Future<void> launchUrl(Uri url) => url_launcher.launchUrl(url);
}
