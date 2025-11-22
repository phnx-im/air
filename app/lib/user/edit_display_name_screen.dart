// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class EditDisplayNameScreen extends HookWidget {
  const EditDisplayNameScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final (displayName, profilePicture) = context.select(
      (UsersCubit cubit) =>
          (cubit.state.displayName(), cubit.state.profilePicture()),
    );
    final userId = context.select((UserCubit cubit) => cubit.state.userId);

    final loc = AppLocalizations.of(context);

    final controller = useTextEditingController(text: displayName);

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.editDisplayNameScreen_title),
        toolbarHeight: isPointer() ? 100 : null,
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints: isPointer()
                ? const BoxConstraints(maxWidth: 800)
                : null,
            padding: const EdgeInsets.all(Spacings.s),
            child: Column(
              children: [
                UserAvatar(userId: userId, size: 100),
                const SizedBox(height: Spacings.m),
                TextFormField(
                  autofocus: true,
                  controller: controller,
                  decoration: InputDecoration(
                    hintText: loc.userHandleScreen_inputHint,
                  ),
                  onFieldSubmitted: (text) => _submit(context, text),
                ),
                const SizedBox(height: Spacings.s),
                Align(
                  alignment: Alignment.centerLeft,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Spacings.xxs,
                    ),
                    child: Text(
                      style: TextStyle(color: Theme.of(context).hintColor),
                      loc.editDisplayNameScreen_description,
                    ),
                  ),
                ),
                const Spacer(),
                OutlinedButton(
                  onPressed: () => _submit(context, controller.text),
                  child: Text(loc.editDisplayNameScreen_save),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _submit(BuildContext context, String text) async {
    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();
    await userCubit.setProfile(displayName: text.trim());
    navigationCubit.pop();
  }
}
