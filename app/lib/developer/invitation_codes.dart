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
  List<InvitationCode> _invitationCodes = [];
  bool _loaded = false;

  @override
  void initState() {
    super.initState();
    if (_loaded) return;
    _loaded = true;
    _loadInvitationCodes();
  }

  void _loadInvitationCodes() async {
    final user = context.read<LoadableUserCubit>().state.loadedUser;
    if (user == null) return;
    final codes = await replenishInvitationCodes(userId: user.userId);
    setState(() => _invitationCodes = codes);
  }

  @override
  Widget build(BuildContext context) {
    return InvitationCodesView(invitationCodes: _invitationCodes);
  }
}

const _maxDesktopWidth = 800.0;

class InvitationCodesView extends StatefulWidget {
  const InvitationCodesView({super.key, required this.invitationCodes});

  final List<InvitationCode> invitationCodes;

  @override
  State<InvitationCodesView> createState() => _InvitationCodesViewState();
}

class _InvitationCodesViewState extends State<InvitationCodesView> {
  final Set<int> _usedIndices = {};

  @override
  Widget build(BuildContext context) {
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
          child: ListView.builder(
            itemCount: widget.invitationCodes.length,
            itemBuilder: (context, index) {
              final code = widget.invitationCodes[index].code;
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
      ),
    );
  }
}
