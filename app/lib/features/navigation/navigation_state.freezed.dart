// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'navigation_state.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$NavigationState {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is NavigationState);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'NavigationState()';
}


}

/// @nodoc
class $NavigationStateCopyWith<$Res>  {
$NavigationStateCopyWith(NavigationState _, $Res Function(NavigationState) __);
}



/// @nodoc


class IntroState extends NavigationState {
  const IntroState({final  List<IntroScreenType> screens = const <IntroScreenType>[]}): _screens = screens,super._();
  

 final  List<IntroScreenType> _screens;
@JsonKey() List<IntroScreenType> get screens {
  if (_screens is EqualUnmodifiableListView) return _screens;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_screens);
}


/// Create a copy of NavigationState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$IntroStateCopyWith<IntroState> get copyWith => _$IntroStateCopyWithImpl<IntroState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is IntroState&&const DeepCollectionEquality().equals(other._screens, _screens));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_screens));

@override
String toString() {
  return 'NavigationState.intro(screens: $screens)';
}


}

/// @nodoc
abstract mixin class $IntroStateCopyWith<$Res> implements $NavigationStateCopyWith<$Res> {
  factory $IntroStateCopyWith(IntroState value, $Res Function(IntroState) _then) = _$IntroStateCopyWithImpl;
@useResult
$Res call({
 List<IntroScreenType> screens
});




}
/// @nodoc
class _$IntroStateCopyWithImpl<$Res>
    implements $IntroStateCopyWith<$Res> {
  _$IntroStateCopyWithImpl(this._self, this._then);

  final IntroState _self;
  final $Res Function(IntroState) _then;

/// Create a copy of NavigationState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? screens = null,}) {
  return _then(IntroState(
screens: null == screens ? _self._screens : screens // ignore: cast_nullable_to_non_nullable
as List<IntroScreenType>,
  ));
}


}

/// @nodoc


class HomeState extends NavigationState {
  const HomeState({this.home = const HomeNavigationState()}): super._();
  

@JsonKey() final  HomeNavigationState home;

/// Create a copy of NavigationState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$HomeStateCopyWith<HomeState> get copyWith => _$HomeStateCopyWithImpl<HomeState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HomeState&&(identical(other.home, home) || other.home == home));
}


@override
int get hashCode => Object.hash(runtimeType,home);

@override
String toString() {
  return 'NavigationState.home(home: $home)';
}


}

/// @nodoc
abstract mixin class $HomeStateCopyWith<$Res> implements $NavigationStateCopyWith<$Res> {
  factory $HomeStateCopyWith(HomeState value, $Res Function(HomeState) _then) = _$HomeStateCopyWithImpl;
@useResult
$Res call({
 HomeNavigationState home
});


$HomeNavigationStateCopyWith<$Res> get home;

}
/// @nodoc
class _$HomeStateCopyWithImpl<$Res>
    implements $HomeStateCopyWith<$Res> {
  _$HomeStateCopyWithImpl(this._self, this._then);

  final HomeState _self;
  final $Res Function(HomeState) _then;

/// Create a copy of NavigationState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? home = null,}) {
  return _then(HomeState(
home: null == home ? _self.home : home // ignore: cast_nullable_to_non_nullable
as HomeNavigationState,
  ));
}

/// Create a copy of NavigationState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$HomeNavigationStateCopyWith<$Res> get home {
  
  return $HomeNavigationStateCopyWith<$Res>(_self.home, (value) {
    return _then(_self.copyWith(home: value));
  });
}
}

/// @nodoc
mixin _$HomeNavigationState {

/// Whether a chat is open, independently of [chatId]: a chat can close
/// without dropping which chat it was.
 bool get chatOpen; ChatId? get chatId; HomeTab get activeTab;/// The open section of the profile tab. `null` is the section list, for
/// which the two-pane layout substitutes [YouSection.profile].
 YouSection? get youSection;/// The chat details drill-down, bottom level first. Empty means closed.
 List<ChatDetailsPage> get chatDetails; bool get createGroupOpen;/// Content the Android share activity handed over, waiting for the open
/// chat's composer to take it. Never set on iOS or desktop.
 PendingShare? get pendingShare;/// The destination picker, shown when the share named no chat.
 bool get shareDestinationOpen;
/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$HomeNavigationStateCopyWith<HomeNavigationState> get copyWith => _$HomeNavigationStateCopyWithImpl<HomeNavigationState>(this as HomeNavigationState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HomeNavigationState&&(identical(other.chatOpen, chatOpen) || other.chatOpen == chatOpen)&&(identical(other.chatId, chatId) || other.chatId == chatId)&&(identical(other.activeTab, activeTab) || other.activeTab == activeTab)&&(identical(other.youSection, youSection) || other.youSection == youSection)&&const DeepCollectionEquality().equals(other.chatDetails, chatDetails)&&(identical(other.createGroupOpen, createGroupOpen) || other.createGroupOpen == createGroupOpen)&&(identical(other.pendingShare, pendingShare) || other.pendingShare == pendingShare)&&(identical(other.shareDestinationOpen, shareDestinationOpen) || other.shareDestinationOpen == shareDestinationOpen));
}


@override
int get hashCode => Object.hash(runtimeType,chatOpen,chatId,activeTab,youSection,const DeepCollectionEquality().hash(chatDetails),createGroupOpen,pendingShare,shareDestinationOpen);

@override
String toString() {
  return 'HomeNavigationState(chatOpen: $chatOpen, chatId: $chatId, activeTab: $activeTab, youSection: $youSection, chatDetails: $chatDetails, createGroupOpen: $createGroupOpen, pendingShare: $pendingShare, shareDestinationOpen: $shareDestinationOpen)';
}


}

/// @nodoc
abstract mixin class $HomeNavigationStateCopyWith<$Res>  {
  factory $HomeNavigationStateCopyWith(HomeNavigationState value, $Res Function(HomeNavigationState) _then) = _$HomeNavigationStateCopyWithImpl;
@useResult
$Res call({
 bool chatOpen, ChatId? chatId, HomeTab activeTab, YouSection? youSection, List<ChatDetailsPage> chatDetails, bool createGroupOpen, PendingShare? pendingShare, bool shareDestinationOpen
});


$PendingShareCopyWith<$Res>? get pendingShare;

}
/// @nodoc
class _$HomeNavigationStateCopyWithImpl<$Res>
    implements $HomeNavigationStateCopyWith<$Res> {
  _$HomeNavigationStateCopyWithImpl(this._self, this._then);

  final HomeNavigationState _self;
  final $Res Function(HomeNavigationState) _then;

/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? chatOpen = null,Object? chatId = freezed,Object? activeTab = null,Object? youSection = freezed,Object? chatDetails = null,Object? createGroupOpen = null,Object? pendingShare = freezed,Object? shareDestinationOpen = null,}) {
  return _then(_self.copyWith(
chatOpen: null == chatOpen ? _self.chatOpen : chatOpen // ignore: cast_nullable_to_non_nullable
as bool,chatId: freezed == chatId ? _self.chatId : chatId // ignore: cast_nullable_to_non_nullable
as ChatId?,activeTab: null == activeTab ? _self.activeTab : activeTab // ignore: cast_nullable_to_non_nullable
as HomeTab,youSection: freezed == youSection ? _self.youSection : youSection // ignore: cast_nullable_to_non_nullable
as YouSection?,chatDetails: null == chatDetails ? _self.chatDetails : chatDetails // ignore: cast_nullable_to_non_nullable
as List<ChatDetailsPage>,createGroupOpen: null == createGroupOpen ? _self.createGroupOpen : createGroupOpen // ignore: cast_nullable_to_non_nullable
as bool,pendingShare: freezed == pendingShare ? _self.pendingShare : pendingShare // ignore: cast_nullable_to_non_nullable
as PendingShare?,shareDestinationOpen: null == shareDestinationOpen ? _self.shareDestinationOpen : shareDestinationOpen // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}
/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$PendingShareCopyWith<$Res>? get pendingShare {
    if (_self.pendingShare == null) {
    return null;
  }

  return $PendingShareCopyWith<$Res>(_self.pendingShare!, (value) {
    return _then(_self.copyWith(pendingShare: value));
  });
}
}



/// @nodoc


class _HomeNavigationState implements HomeNavigationState {
  const _HomeNavigationState({this.chatOpen = false, this.chatId, this.activeTab = HomeTab.chats, this.youSection, final  List<ChatDetailsPage> chatDetails = const <ChatDetailsPage>[], this.createGroupOpen = false, this.pendingShare, this.shareDestinationOpen = false}): _chatDetails = chatDetails;
  

/// Whether a chat is open, independently of [chatId]: a chat can close
/// without dropping which chat it was.
@override@JsonKey() final  bool chatOpen;
@override final  ChatId? chatId;
@override@JsonKey() final  HomeTab activeTab;
/// The open section of the profile tab. `null` is the section list, for
/// which the two-pane layout substitutes [YouSection.profile].
@override final  YouSection? youSection;
/// The chat details drill-down, bottom level first. Empty means closed.
 final  List<ChatDetailsPage> _chatDetails;
/// The chat details drill-down, bottom level first. Empty means closed.
@override@JsonKey() List<ChatDetailsPage> get chatDetails {
  if (_chatDetails is EqualUnmodifiableListView) return _chatDetails;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_chatDetails);
}

@override@JsonKey() final  bool createGroupOpen;
/// Content the Android share activity handed over, waiting for the open
/// chat's composer to take it. Never set on iOS or desktop.
@override final  PendingShare? pendingShare;
/// The destination picker, shown when the share named no chat.
@override@JsonKey() final  bool shareDestinationOpen;

/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$HomeNavigationStateCopyWith<_HomeNavigationState> get copyWith => __$HomeNavigationStateCopyWithImpl<_HomeNavigationState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _HomeNavigationState&&(identical(other.chatOpen, chatOpen) || other.chatOpen == chatOpen)&&(identical(other.chatId, chatId) || other.chatId == chatId)&&(identical(other.activeTab, activeTab) || other.activeTab == activeTab)&&(identical(other.youSection, youSection) || other.youSection == youSection)&&const DeepCollectionEquality().equals(other._chatDetails, _chatDetails)&&(identical(other.createGroupOpen, createGroupOpen) || other.createGroupOpen == createGroupOpen)&&(identical(other.pendingShare, pendingShare) || other.pendingShare == pendingShare)&&(identical(other.shareDestinationOpen, shareDestinationOpen) || other.shareDestinationOpen == shareDestinationOpen));
}


@override
int get hashCode => Object.hash(runtimeType,chatOpen,chatId,activeTab,youSection,const DeepCollectionEquality().hash(_chatDetails),createGroupOpen,pendingShare,shareDestinationOpen);

@override
String toString() {
  return 'HomeNavigationState(chatOpen: $chatOpen, chatId: $chatId, activeTab: $activeTab, youSection: $youSection, chatDetails: $chatDetails, createGroupOpen: $createGroupOpen, pendingShare: $pendingShare, shareDestinationOpen: $shareDestinationOpen)';
}


}

/// @nodoc
abstract mixin class _$HomeNavigationStateCopyWith<$Res> implements $HomeNavigationStateCopyWith<$Res> {
  factory _$HomeNavigationStateCopyWith(_HomeNavigationState value, $Res Function(_HomeNavigationState) _then) = __$HomeNavigationStateCopyWithImpl;
@override @useResult
$Res call({
 bool chatOpen, ChatId? chatId, HomeTab activeTab, YouSection? youSection, List<ChatDetailsPage> chatDetails, bool createGroupOpen, PendingShare? pendingShare, bool shareDestinationOpen
});


@override $PendingShareCopyWith<$Res>? get pendingShare;

}
/// @nodoc
class __$HomeNavigationStateCopyWithImpl<$Res>
    implements _$HomeNavigationStateCopyWith<$Res> {
  __$HomeNavigationStateCopyWithImpl(this._self, this._then);

  final _HomeNavigationState _self;
  final $Res Function(_HomeNavigationState) _then;

/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? chatOpen = null,Object? chatId = freezed,Object? activeTab = null,Object? youSection = freezed,Object? chatDetails = null,Object? createGroupOpen = null,Object? pendingShare = freezed,Object? shareDestinationOpen = null,}) {
  return _then(_HomeNavigationState(
chatOpen: null == chatOpen ? _self.chatOpen : chatOpen // ignore: cast_nullable_to_non_nullable
as bool,chatId: freezed == chatId ? _self.chatId : chatId // ignore: cast_nullable_to_non_nullable
as ChatId?,activeTab: null == activeTab ? _self.activeTab : activeTab // ignore: cast_nullable_to_non_nullable
as HomeTab,youSection: freezed == youSection ? _self.youSection : youSection // ignore: cast_nullable_to_non_nullable
as YouSection?,chatDetails: null == chatDetails ? _self._chatDetails : chatDetails // ignore: cast_nullable_to_non_nullable
as List<ChatDetailsPage>,createGroupOpen: null == createGroupOpen ? _self.createGroupOpen : createGroupOpen // ignore: cast_nullable_to_non_nullable
as bool,pendingShare: freezed == pendingShare ? _self.pendingShare : pendingShare // ignore: cast_nullable_to_non_nullable
as PendingShare?,shareDestinationOpen: null == shareDestinationOpen ? _self.shareDestinationOpen : shareDestinationOpen // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

/// Create a copy of HomeNavigationState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$PendingShareCopyWith<$Res>? get pendingShare {
    if (_self.pendingShare == null) {
    return null;
  }

  return $PendingShareCopyWith<$Res>(_self.pendingShare!, (value) {
    return _then(_self.copyWith(pendingShare: value));
  });
}
}

/// @nodoc
mixin _$ChatDetailsPage {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatDetailsPage);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatDetailsPage()';
}


}

/// @nodoc
class $ChatDetailsPageCopyWith<$Res>  {
$ChatDetailsPageCopyWith(ChatDetailsPage _, $Res Function(ChatDetailsPage) __);
}



/// @nodoc


class DetailsPage implements ChatDetailsPage {
  const DetailsPage();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is DetailsPage);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatDetailsPage.details()';
}


}




/// @nodoc


class GroupMembersPage implements ChatDetailsPage {
  const GroupMembersPage();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is GroupMembersPage);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatDetailsPage.groupMembers()';
}


}




/// @nodoc


class AddMembersPage implements ChatDetailsPage {
  const AddMembersPage();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AddMembersPage);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatDetailsPage.addMembers()';
}


}




/// @nodoc


class MemberDetailsPage implements ChatDetailsPage {
  const MemberDetailsPage(this.member);
  

 final  UiUserId member;

/// Create a copy of ChatDetailsPage
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MemberDetailsPageCopyWith<MemberDetailsPage> get copyWith => _$MemberDetailsPageCopyWithImpl<MemberDetailsPage>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MemberDetailsPage&&(identical(other.member, member) || other.member == member));
}


@override
int get hashCode => Object.hash(runtimeType,member);

@override
String toString() {
  return 'ChatDetailsPage.memberDetails(member: $member)';
}


}

/// @nodoc
abstract mixin class $MemberDetailsPageCopyWith<$Res> implements $ChatDetailsPageCopyWith<$Res> {
  factory $MemberDetailsPageCopyWith(MemberDetailsPage value, $Res Function(MemberDetailsPage) _then) = _$MemberDetailsPageCopyWithImpl;
@useResult
$Res call({
 UiUserId member
});




}
/// @nodoc
class _$MemberDetailsPageCopyWithImpl<$Res>
    implements $MemberDetailsPageCopyWith<$Res> {
  _$MemberDetailsPageCopyWithImpl(this._self, this._then);

  final MemberDetailsPage _self;
  final $Res Function(MemberDetailsPage) _then;

/// Create a copy of ChatDetailsPage
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? member = null,}) {
  return _then(MemberDetailsPage(
null == member ? _self.member : member // ignore: cast_nullable_to_non_nullable
as UiUserId,
  ));
}


}

/// @nodoc


class SafetyCodePage implements ChatDetailsPage {
  const SafetyCodePage(this.user);
  

 final  UiUserId user;

/// Create a copy of ChatDetailsPage
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SafetyCodePageCopyWith<SafetyCodePage> get copyWith => _$SafetyCodePageCopyWithImpl<SafetyCodePage>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SafetyCodePage&&(identical(other.user, user) || other.user == user));
}


@override
int get hashCode => Object.hash(runtimeType,user);

@override
String toString() {
  return 'ChatDetailsPage.safetyCode(user: $user)';
}


}

/// @nodoc
abstract mixin class $SafetyCodePageCopyWith<$Res> implements $ChatDetailsPageCopyWith<$Res> {
  factory $SafetyCodePageCopyWith(SafetyCodePage value, $Res Function(SafetyCodePage) _then) = _$SafetyCodePageCopyWithImpl;
@useResult
$Res call({
 UiUserId user
});




}
/// @nodoc
class _$SafetyCodePageCopyWithImpl<$Res>
    implements $SafetyCodePageCopyWith<$Res> {
  _$SafetyCodePageCopyWithImpl(this._self, this._then);

  final SafetyCodePage _self;
  final $Res Function(SafetyCodePage) _then;

/// Create a copy of ChatDetailsPage
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? user = null,}) {
  return _then(SafetyCodePage(
null == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as UiUserId,
  ));
}


}

// dart format on
