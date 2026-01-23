// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/attachments/attachments.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';

const physicalSize = Size(800, 1800);

final file = UiAttachment(
  attachmentId: 42.attachmentId(),
  filename: 'example.bin',
  contentType: 'application/octet-stream',
  size: 10 * 1024 * 1024,
);

List<(Color, Color)> testColors(BuildContext context) {
  final colors = CustomColorScheme.of(context);
  return [
    (colors.message.selfText, colors.message.selfBackground),
    (colors.message.otherText, colors.message.otherBackground),
  ];
}

Map<AttachmentId, UiAttachmentStatus> testStatuses = {
  1.attachmentId(): const UiAttachmentStatus.pending(),
  2.attachmentId(): UiAttachmentStatus.progress(BigInt.from(7 * 1024 * 1024)),
  3.attachmentId(): const UiAttachmentStatus.failed(),
  4.attachmentId(): const UiAttachmentStatus.completed(),
};

class _FileTestBubble extends StatelessWidget {
  const _FileTestBubble({
    required this.attachmentId,
    required this.color,
    required this.backgroundColor,
    this.attachment,
  });

  final AttachmentId attachmentId;
  final Color color;
  final Color backgroundColor;
  final UiAttachment? attachment;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.s,
        vertical: Spacings.xxs,
      ),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(Spacings.sm),
      ),
      child: AttachmentFile(
        attachment: attachment ?? file.copyWith(attachmentId: attachmentId),
        isSender: true,
        color: color,
      ),
    );
  }
}

void main() {
  setUpAll(() {
    registerFallbackValue(0.attachmentId());
  });

  late MockAttachmentsRepository attachmentsRepository;

  setUp(() async {
    attachmentsRepository = MockAttachmentsRepository();
    when(
      () => attachmentsRepository.statusStream(
        attachmentId: any(named: 'attachmentId'),
      ),
    ).thenAnswer(
      (invocation) => Stream.value(
        testStatuses[invocation.namedArguments[#attachmentId]
                as AttachmentId] ??
            const UiAttachmentStatus.completed(),
      ),
    );
  });

  group('AttachmentFile', () {
    Widget buildSubject(List<Widget> Function(BuildContext) children) =>
        Builder(
          builder: (context) {
            return RepositoryProvider<AttachmentsRepository>.value(
              value: attachmentsRepository,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: themeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Scaffold(
                  body: Padding(
                    padding: const EdgeInsets.all(Spacings.s),
                    child: SizedBox(
                      width: double.infinity,
                      child: Column(
                        spacing: Spacings.s,
                        crossAxisAlignment: .end,
                        children: children(context),
                      ),
                    ),
                  ),
                ),
              ),
            );
          },
        );

    List<_FileTestBubble> attachmentPerStatus(context) => [
      for (final (color, backgroundColor) in testColors(context))
        for (final attachmentId in testStatuses.keys) ...[
          _FileTestBubble(
            attachmentId: attachmentId,
            color: color,
            backgroundColor: backgroundColor,
          ),
        ],
    ];

    testWidgets('renders correctly', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject(attachmentPerStatus));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject(attachmentPerStatus));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file_dark_mode.png'),
      );
    });

    testWidgets('renders correctly overflow', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      List<_FileTestBubble> children(context) => [
        for (final (color, backgroundColor) in testColors(context))
          _FileTestBubble(
            attachmentId: 5.attachmentId(),
            color: color,
            backgroundColor: backgroundColor,
            attachment: file.copyWith(
              filename:
                  'a_very_logn_long_filename_which_should_break_the_text.bin',
            ),
          ),
      ];

      await tester.pumpWidget(buildSubject(children));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file_overflow.png'),
      );
    });
  });
}
