// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

class AddUserHandleScreen extends HookWidget {
  const AddUserHandleScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final formKey = useMemoized(() => GlobalKey<FormState>());

    final userHandleExists = useState(false);
    final isSubmitting = useState(false);

    final controller = useTextEditingController();

    final focusNode = useFocusNode();

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.userHandleScreen_title),
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
            child: Form(
              key: formKey,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const _UserHandleAvatar(),
                  const SizedBox(height: Spacings.m),
                  TextFormField(
                    autocorrect: true,
                    autofocus: true,
                    controller: controller,
                    focusNode: focusNode,
                    decoration: InputDecoration(
                      hintText: loc.userHandleScreen_inputHint,
                    ),
                    // Temporary strict enforcement until legacy underscores are fully removed.
                    inputFormatters: const [UserHandleInputFormatter()],
                    validator: (value) =>
                        _validate(loc, userHandleExists, value),
                    onChanged: (_) {
                      if (userHandleExists.value) {
                        userHandleExists.value = false;
                        formKey.currentState!.validate();
                      }
                    },
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
                  const SizedBox(height: Spacings.s),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: Spacings.xxs,
                      ),
                      child: Text(
                        style: TextStyle(color: Theme.of(context).hintColor),
                        loc.userHandleScreen_description,
                      ),
                    ),
                  ),
                  const Spacer(),
                  OutlinedButton(
                    onPressed: () => _submit(
                      context,
                      formKey,
                      controller,
                      userHandleExists,
                      isSubmitting,
                    ),
                    child: !isSubmitting.value
                        ? Text(loc.userHandleScreen_save)
                        : CircularProgressIndicator(
                            valueColor: AlwaysStoppedAnimation<Color>(
                              CustomColorScheme.of(context).text.secondary,
                            ),
                            backgroundColor: Colors.transparent,
                          ),
                  ),
                ],
              ),
            ),
          ),
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
    final navigationCubit = context.read<NavigationCubit>();

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
    navigationCubit.pop();
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

class _UserHandleAvatar extends StatelessWidget {
  const _UserHandleAvatar();

  static const _size = 100.0;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: _size,
      height: _size,
      child: CircleAvatar(
        radius: _size / 2,
        backgroundColor: CustomColorScheme.of(context).text.quaternary,
        child: const Icon(Icons.alternate_email, size: _size / 1.5),
      ),
    );
  }
}
