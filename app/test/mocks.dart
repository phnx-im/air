// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/message_list/message_cubit.dart';
import 'package:air/message_list/message_list_cubit.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/registration.dart';
import 'package:air/user/user.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:mocktail/mocktail.dart';

import 'helpers.dart';

class MockNavigationCubit extends MockCubit<NavigationState>
    implements NavigationCubit {}

class MockUserCubit extends MockCubit<UiUser> implements UserCubit {
  @override
  AppState get appState => AppState.foreground;
}

class MockUsersCubit extends MockCubit<UsersState> implements UsersCubit {}

class MockUiUser implements UiUser {
  MockUiUser({required int id, List<UiUserHandle> userHandles = const []})
    : _userId = id.userId(),
      _userHandles = userHandles;

  final UiUserId _userId;
  final List<UiUserHandle> _userHandles;

  @override
  UiUserId get userId => _userId;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;

  @override
  List<UiUserHandle> get userHandles => _userHandles;

  @override
  bool get unsupportedVersion => false;
}

class MockUsersState implements UsersState {
  MockUsersState({
    UiUserId? defaultUserId,
    required List<UiUserProfile> profiles,
  }) : _defaultUserId = defaultUserId ?? 1.userId(),
       _profiles = {for (final profile in profiles) profile.userId: profile};

  final UiUserId _defaultUserId;
  final Map<UiUserId, UiUserProfile> _profiles;

  @override
  UiUserProfile profile({UiUserId? userId}) {
    final id = userId ?? _defaultUserId;
    return _profiles[id]!;
  }

  @override
  String displayName({UiUserId? userId}) => profile(userId: userId).displayName;

  @override
  ImageData? profilePicture({UiUserId? userId}) =>
      profile(userId: userId).profilePicture;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;
}

class MockChatDetailsCubit extends MockCubit<ChatDetailsState>
    implements ChatDetailsCubit {}

class MockChatListCubit extends MockCubit<ChatListState>
    implements ChatListCubit {}

class MockMessageListCubit extends MockCubit<MessageListState>
    implements MessageListCubit {}

class MockMessageListState implements MessageListState {
  MockMessageListState(this.messages, {this.isConnectionChat = false});

  final List<UiChatMessage> messages;

  @override
  final bool isConnectionChat;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;

  @override
  int get loadedMessagesCount => messages.length;

  @override
  UiChatMessage? messageAt(int index) => messages.elementAtOrNull(index);

  @override
  int? messageIdIndex(MessageId messageId) {
    final index = messages.indexWhere((element) => element.id == messageId);
    return index != -1 ? index : null;
  }

  @override
  bool isNewMessage(MessageId messageId) {
    return false;
  }
}

class MockMessageCubit extends MockCubit<MessageState> implements MessageCubit {
  MockMessageCubit({required MessageState initialState}) {
    when(() => state).thenReturn(initialState);
  }
}

class MockLoadableUserCubit extends MockCubit<LoadableUser>
    implements LoadableUserCubit {}

class MockUser extends Mock implements User {}

class MockRegistrationCubit extends MockCubit<RegistrationState>
    implements RegistrationCubit {}

class MockAttachmentsRepository extends Mock implements AttachmentsRepository {}

class MockUserSettingsCubit extends MockCubit<UserSettings>
    implements UserSettingsCubit {}

class MockChatsRepository extends Mock implements ChatsRepository {}

class MockMemberDetailsCubit extends MockCubit<MemberDetailsState>
    implements MemberDetailsCubit {}
