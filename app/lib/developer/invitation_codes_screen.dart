// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/user/invitation_codes_cubit.dart';
import 'package:air/user/user.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';

class InvitationCodesScreen extends StatelessWidget {
  const InvitationCodesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider<InvitationCodesCubit>(
      create: (BuildContext context) {
        final userCubit = context.read<UserCubit>();
        return InvitationCodesCubit(userCubit: userCubit);
      },
      child: const InvitationCodesView(),
    );
  }
}

const _maxDesktopWidth = 800.0;

class InvitationCodesView extends StatelessWidget {
  const InvitationCodesView({super.key});

  @override
  Widget build(BuildContext context) {
    final invitationCodes = context.select(
      (InvitationCodesCubit cubit) => cubit.state.codes,
    );

    return Scaffold(
      appBar: AppBar(
        title: const Text('Invitation Codes'),
        toolbarHeight: isPointer() ? 100 : null,
        leading: const AppBarBackButton(),
      ),
      body: Center(
        child: Container(
          constraints: isPointer()
              ? const BoxConstraints(maxWidth: _maxDesktopWidth)
              : null,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              GestureDetector(
                onTap: () {
                  final user = context
                      .read<LoadableUserCubit>()
                      .state
                      .loadedUser;

                  Clipboard.setData(
                    ClipboardData(
                      text: "${user?.userId.uuid}@${user?.userId.domain}",
                    ),
                  );
                },
                child: Text(
                  "${invitationCodes.length} invitation codes available to share.",
                ),
              ),
              Expanded(
                child: ListView.builder(
                  itemCount: invitationCodes.length,
                  itemBuilder: (context, index) {
                    final code = invitationCodes[index];
                    return ListTile(
                      title: Text(
                        code.code,
                        style: code.copied
                            ? const TextStyle(
                                decoration: TextDecoration.lineThrough,
                                decorationThickness: 2,
                              )
                            : null,
                      ),
                      trailing: IconButton(
                        icon: const Icon(Icons.copy),
                        onPressed: () {
                          Clipboard.setData(ClipboardData(text: code.code));
                          context
                              .read<InvitationCodesCubit>()
                              .markInvitationCodeAsCopied(code: code.code);
                        },
                      ),
                    );
                  },
                ),
              ),
              AppButton(
                onPressed: () async {
                  final loc = AppLocalizations.of(context);
                  final error = await context
                      .read<InvitationCodesCubit>()
                      .requestInvitationCode();
                  if (error == null) {
                    return;
                  }

                  final message = switch (error) {
                    .globalQuotaExceeded =>
                      loc.invitationCodesScreen_global_quota_exceeded,
                    .userQuotaExceeded =>
                      loc.invitationCodesScreen_user_quota_exceeded,
                  };

                  showSnackBarStandalone(
                    (_) => SnackBar(content: Text(message)),
                  );
                },
                label: "Request invitation code",
              ),
            ],
          ),
        ),
      ),
    );
  }
}
