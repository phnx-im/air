// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'member_details_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$MemberDetailsState {

 List<UiUserId> get members; UiRoomState? get roomState;
/// Create a copy of MemberDetailsState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MemberDetailsStateCopyWith<MemberDetailsState> get copyWith => _$MemberDetailsStateCopyWithImpl<MemberDetailsState>(this as MemberDetailsState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MemberDetailsState&&const DeepCollectionEquality().equals(other.members, members)&&(identical(other.roomState, roomState) || other.roomState == roomState));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(members),roomState);

@override
String toString() {
  return 'MemberDetailsState(members: $members, roomState: $roomState)';
}


}

/// @nodoc
abstract mixin class $MemberDetailsStateCopyWith<$Res>  {
  factory $MemberDetailsStateCopyWith(MemberDetailsState value, $Res Function(MemberDetailsState) _then) = _$MemberDetailsStateCopyWithImpl;
@useResult
$Res call({
 List<UiUserId> members, UiRoomState? roomState
});




}
/// @nodoc
class _$MemberDetailsStateCopyWithImpl<$Res>
    implements $MemberDetailsStateCopyWith<$Res> {
  _$MemberDetailsStateCopyWithImpl(this._self, this._then);

  final MemberDetailsState _self;
  final $Res Function(MemberDetailsState) _then;

/// Create a copy of MemberDetailsState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? members = null,Object? roomState = freezed,}) {
  return _then(_self.copyWith(
members: null == members ? _self.members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,roomState: freezed == roomState ? _self.roomState : roomState // ignore: cast_nullable_to_non_nullable
as UiRoomState?,
  ));
}

}



/// @nodoc


class _MemberDetailsState extends MemberDetailsState {
  const _MemberDetailsState({required final  List<UiUserId> members, this.roomState}): _members = members,super._();
  

 final  List<UiUserId> _members;
@override List<UiUserId> get members {
  if (_members is EqualUnmodifiableListView) return _members;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_members);
}

@override final  UiRoomState? roomState;

/// Create a copy of MemberDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$MemberDetailsStateCopyWith<_MemberDetailsState> get copyWith => __$MemberDetailsStateCopyWithImpl<_MemberDetailsState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _MemberDetailsState&&const DeepCollectionEquality().equals(other._members, _members)&&(identical(other.roomState, roomState) || other.roomState == roomState));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_members),roomState);

@override
String toString() {
  return 'MemberDetailsState(members: $members, roomState: $roomState)';
}


}

/// @nodoc
abstract mixin class _$MemberDetailsStateCopyWith<$Res> implements $MemberDetailsStateCopyWith<$Res> {
  factory _$MemberDetailsStateCopyWith(_MemberDetailsState value, $Res Function(_MemberDetailsState) _then) = __$MemberDetailsStateCopyWithImpl;
@override @useResult
$Res call({
 List<UiUserId> members, UiRoomState? roomState
});




}
/// @nodoc
class __$MemberDetailsStateCopyWithImpl<$Res>
    implements _$MemberDetailsStateCopyWith<$Res> {
  __$MemberDetailsStateCopyWithImpl(this._self, this._then);

  final _MemberDetailsState _self;
  final $Res Function(_MemberDetailsState) _then;

/// Create a copy of MemberDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? members = null,Object? roomState = freezed,}) {
  return _then(_MemberDetailsState(
members: null == members ? _self._members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,roomState: freezed == roomState ? _self.roomState : roomState // ignore: cast_nullable_to_non_nullable
as UiRoomState?,
  ));
}


}

// dart format on
