// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter/services.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';

class InvitationCodesScreen extends StatefulWidget {
  const InvitationCodesScreen({super.key});

  @override
  State<InvitationCodesScreen> createState() => _InvitationCodesState();
}

class _InvitationCodesState extends State<InvitationCodesScreen> {
  Future<List<InvitationCode>>? _invitationCodes;

  @override
  void initState() {
    super.initState();
    _loadInvitationCodes();
  }

  void _loadInvitationCodes() {
    final user = context.read<LoadableUserCubit>().state.loadedUser;
    if (user == null) return;
    final invitationCodes = []; //replenishInvitationCodes(userId: user.userId);
    setState(() {
      _invitationCodes = invitationCodes;
    });
  }

  @override
  Widget build(BuildContext context) {
    return InvitationCodesView(invitationCodes: _invitationCodes);
  }
}

const _maxDesktopWidth = 800.0;

class InvitationCodesView extends StatefulWidget {
  const InvitationCodesView({super.key, required this.invitationCodes});

  final Future<List<InvitationCode>>? invitationCodes;

  @override
  State<InvitationCodesView> createState() => _InvitationCodesViewState();
}

class _InvitationCodesViewState extends State<InvitationCodesView> {
  final Set<int> _usedIndices = {};

  @override
  Widget build(BuildContext context) {
    final user = context.read<LoadableUserCubit>().state.loadedUser;

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
          child: FutureBuilder(
            future: widget.invitationCodes,
            builder: (context, snapshot) {
              if (snapshot.hasData) {
                final invitationCodes = snapshot.data!;
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    GestureDetector(
                      onTap: () {
                        Clipboard.setData(
                          ClipboardData(
                            text: "${user?.userId.uuid}@${user?.userId.domain}",
                          ),
                        );
                      },
                      child: Text(
                        "${invitationCodes.length} invitation codes available to share for ${user?.userId.uuid}@${user?.userId.domain}.",
                      ),
                    ),
                    Expanded(
                      child: ListView.builder(
                        itemCount: invitationCodes.length,
                        itemBuilder: (context, index) {
                          final code = invitationCodes[index].code;
                          final used = _usedIndices.contains(index);
                          return ListTile(
                            title: Text(
                              code,
                              style: used
                                  ? const TextStyle(
                                      decoration: TextDecoration.lineThrough,
                                      decorationThickness: 2,
                                    )
                                  : null,
                            ),
                            trailing: IconButton(
                              icon: const Icon(Icons.copy),
                              onPressed: () {
                                Clipboard.setData(ClipboardData(text: code));
                                setState(() => _usedIndices.add(index));
                              },
                            ),
                          );
                        },
                      ),
                    ),
                  ],
                );
              } else if (snapshot.hasError) {
                return const Text("Failed to load invitation codes");
              } else {
                return const CircularProgressIndicator();
              }
            },
          ),
        ),
      ),
    );
  }
}
