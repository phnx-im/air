// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:provider/provider.dart';

class ChangeDisplayNameDialog extends HookWidget {
  const ChangeDisplayNameDialog({super.key, required this.displayName});

  final String displayName;

  @override
  Widget build(BuildContext context) {
    final controller = useTextEditingController();
    useEffect(() {
      controller.text = displayName;
      return null;
    }, [displayName]);

    final focusNode = useFocusNode();

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.editDisplayNameScreen_title,
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
            decoration: appDialogInputDecoration.copyWith(
              filled: true,
              fillColor: colors.backgroundBase.secondary,
            ),
            onFieldSubmitted: (_) {
              focusNode.requestFocus();
              _submit(context, controller.text);
            },
          ),

          const SizedBox(height: Spacings.xs),

          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
            child: Text(
              loc.editDisplayNameScreen_description,
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
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.quaternary,
                    ),
                  ),
                  child: Text(loc.editDisplayNameScreen_cancel),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: OutlinedButton(
                  onPressed: () => _submit(context, controller.text),
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.primary,
                    ),
                    foregroundColor: WidgetStatePropertyAll(
                      colors.function.toggleWhite,
                    ),
                  ),
                  child: Text(loc.editDisplayNameScreen_save),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _submit(BuildContext context, String text) async {
    final userCubit = context.read<UserCubit>();
    userCubit.setProfile(displayName: text.trim());
    Navigator.of(context).pop();
  }
}
