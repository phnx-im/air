// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/util/platform.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:share_plus/share_plus.dart';

class LogsScreen extends StatefulWidget {
  const LogsScreen({super.key});

  @override
  State<LogsScreen> createState() => _LogsScreenState();
}

class _LogsScreenState extends State<LogsScreen> {
  late Future<String> _appLogs;
  late Future<String> _backgroundLogs;
  late Timer _timer;

  @override
  void initState() {
    super.initState();
    _loadLogs();
    _timer = Timer.periodic(const Duration(seconds: 1), (timer) {
      _loadLogs();
    });
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  void _loadLogs() async {
    final appLogs = readAppLogs();
    final backgroundLogs = getCacheDirectory().then(
      (cacheDir) => readBackgroundLogs(cacheDir: cacheDir),
    );

    setState(() {
      _appLogs = appLogs;
      _backgroundLogs = backgroundLogs;
    });
  }

  void _clearLogs() async {
    await clearAppLogs();
    final cacheDir = await getCacheDirectory();
    await clearBackgroundLogs(cacheDir: cacheDir);
    setState(() {
      _appLogs = Future.value("");
      _backgroundLogs = Future.value("");
    });
  }

  @override
  Widget build(BuildContext context) {
    return LogsScreenView(
      appLogs: _appLogs,
      backgroundLogs: _backgroundLogs,
      reloadLogs: _loadLogs,
      clearLogs: _clearLogs,
    );
  }
}

class LogsScreenView extends StatelessWidget {
  const LogsScreenView({
    required this.appLogs,
    required this.backgroundLogs,
    required this.reloadLogs,
    required this.clearLogs,
    super.key,
  });

  final Future<String> appLogs;
  final Future<String> backgroundLogs;
  final VoidCallback reloadLogs;
  final VoidCallback clearLogs;

  @override
  Widget build(BuildContext context) {
    return DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: const Text('Logs'),
          toolbarHeight: isPointer() ? 100 : null,
          leading: const AppBarBackButton(),
          actions: [
            PopupMenuButton(
              itemBuilder:
                  (context) => [
                    if (Platform.isLinux ||
                        Platform.isMacOS ||
                        Platform.isWindows)
                      PopupMenuItem(
                        onTap: _saveLogs,
                        child: const Text('Save'),
                      ),
                    if (Platform.isAndroid || Platform.isIOS)
                      PopupMenuItem(
                        onTap: _shareLogs,
                        child: const Text('Share'),
                      ),
                    PopupMenuItem(
                      onTap: reloadLogs,
                      child: const Text('Reload'),
                    ),
                    PopupMenuItem(onTap: clearLogs, child: const Text('Clear')),
                  ],
            ),
          ],
        ),
        bottomNavigationBar: const SafeArea(
          left: false,
          right: false,
          top: false,
          bottom: true,
          child: TabBar(tabs: [Tab(text: 'App'), Tab(text: 'Background')]),
        ),
        body: SafeArea(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
            child: TabBarView(
              children: [
                _LogsView(logs: appLogs),
                _LogsView(logs: backgroundLogs),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _shareLogs() async {
    final cacheDir = await getCacheDirectory();
    final data = await tarLogs(cacheDir: cacheDir);
    final file = XFile.fromData(data, mimeType: 'application/gzip');
    SharePlus.instance.share(
      ShareParams(files: [file], fileNameOverrides: ['logs.tar.gz']),
    );
  }

  void _saveLogs() async {
    final cacheDir = await getCacheDirectory();
    final data = await tarLogs(cacheDir: cacheDir);

    const String fileName = 'logs.tar.gz';
    final FileSaveLocation? result = await getSaveLocation(
      suggestedName: fileName,
    );
    if (result == null) {
      // Operation was canceled by the user.
      return;
    }

    await XFile.fromData(
      data,
      mimeType: 'application/gzip',
    ).saveTo(result.path);
  }
}

class _LogsView extends StatefulWidget {
  const _LogsView({required this.logs});

  final Future<String>? logs;

  @override
  State<_LogsView> createState() => _LogsViewState();
}

class _LogsViewState extends State<_LogsView>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);
    return FutureBuilder(
      future: widget.logs,
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          final data = snapshot.data!;
          return SelectableText(data);
        } else if (snapshot.hasError) {
          return const Center(child: Text('Error loading logs'));
        }
        return const Center(
          child: SizedBox(child: CircularProgressIndicator()),
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => true;
}
