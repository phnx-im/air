// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

import 'user_cubit.dart';

class AddUsernameDialog extends HookWidget {
  const AddUsernameDialog({super.key, this.inProgress});

  final bool? inProgress;

  @override
  Widget build(BuildContext context) {
    final formKey = useMemoized(() => GlobalKey<FormState>());

    final userHandleExists = useState(false);
    final isSubmitting = useState(false);

    final controller = useTextEditingController();
    final focusNode = useFocusNode();

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return AppDialog(
      child: Form(
        key: formKey,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Text(
                loc.userHandleScreen_title,
                style: TextStyle(
                  fontSize: HeaderFontSize.h4.size,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ),
            const SizedBox(height: Spacings.m),

            TextFormField(
              autocorrect: false,
              autofocus: true,
              controller: controller,
              focusNode: focusNode,
              inputFormatters: const [UserHandleInputFormatter()],
              validator: (value) => _validate(loc, userHandleExists, value),
              onChanged: (_) {
                if (userHandleExists.value) {
                  userHandleExists.value = false;
                  formKey.currentState!.validate();
                }
              },
              decoration: appDialogInputDecoration.copyWith(
                hintText: loc.userHandleScreen_inputHint,
                filled: true,
                fillColor: colors.backgroundBase.secondary,
              ),
              onFieldSubmitted: (_) {
                focusNode.requestFocus();
                _submit(
                  context,
                  formKey,
                  controller,
                  userHandleExists,
                  isSubmitting,
                );
              },
            ),

            const SizedBox(height: Spacings.xs),

            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
              child: Text(
                loc.userHandleScreen_description,
                style: TextStyle(
                  color: colors.text.tertiary,
                  fontSize: BodyFontSize.small2.size,
                ),
              ),
            ),

            const SizedBox(height: Spacings.m),

            Row(
              children: [
                Expanded(
                  child: OutlinedButton(
                    onPressed: () {
                      Navigator.of(context).pop(false);
                    },
                    child: Text(loc.userHandleScreen_cancel),
                  ),
                ),
                const SizedBox(width: Spacings.xs),
                Expanded(
                  child: AppDialogProgressButton(
                    onPressed: (isSubmitting) => _submit(
                      context,
                      formKey,
                      controller,
                      userHandleExists,
                      isSubmitting,
                    ),
                    style: ButtonStyle(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.accent.primary,
                      ),
                      foregroundColor: WidgetStatePropertyAll(
                        colors.function.toggleWhite,
                      ),
                    ),
                    progressColor: colors.function.toggleWhite,
                    inProgress: inProgress,
                    child: Text(loc.userHandleScreen_confirm),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  void _submit(
    BuildContext context,
    GlobalKey<FormState> formKey,
    TextEditingController controller,
    ValueNotifier<bool> alreadyExists,
    ValueNotifier<bool> isSubmitting,
  ) async {
    if (!formKey.currentState!.validate()) {
      return;
    }
    final normalized = UserHandleInputFormatter.normalize(controller.text);
    final handle = UiUserHandle(plaintext: normalized);
    final userCubit = context.read<UserCubit>();

    // Clear already exists if any
    if (alreadyExists.value) {
      alreadyExists.value = false;
      formKey.currentState!.validate();
    }

    isSubmitting.value = true;
    if (!await userCubit.addUserHandle(handle)) {
      alreadyExists.value = true;
      isSubmitting.value = false;
      formKey.currentState!.validate();
      return;
    }
    if (!context.mounted) return;
    Navigator.of(context).pop();
  }

  String? _validate(
    AppLocalizations loc,
    ValueNotifier<bool> userHandleExists,
    String? value,
  ) {
    if (userHandleExists.value) {
      return loc.userHandleScreen_error_alreadyExists;
    }
    if (value == null || value.trim().isEmpty) {
      return loc.userHandleScreen_error_emptyHandle;
    }
    final safeValue = value;
    final normalized = UserHandleInputFormatter.normalize(safeValue);
    if (normalized.isEmpty) {
      return loc.userHandleScreen_error_emptyHandle;
    }
    final handle = UiUserHandle(plaintext: normalized);
    return handle.validationError();
  }
}
