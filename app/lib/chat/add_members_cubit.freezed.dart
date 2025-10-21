// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'add_members_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$AddMembersState {

 List<UiContact> get contacts; Set<UiUserId> get selectedContacts;
/// Create a copy of AddMembersState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AddMembersStateCopyWith<AddMembersState> get copyWith => _$AddMembersStateCopyWithImpl<AddMembersState>(this as AddMembersState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AddMembersState&&const DeepCollectionEquality().equals(other.contacts, contacts)&&const DeepCollectionEquality().equals(other.selectedContacts, selectedContacts));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(contacts),const DeepCollectionEquality().hash(selectedContacts));

@override
String toString() {
  return 'AddMembersState(contacts: $contacts, selectedContacts: $selectedContacts)';
}


}

/// @nodoc
abstract mixin class $AddMembersStateCopyWith<$Res>  {
  factory $AddMembersStateCopyWith(AddMembersState value, $Res Function(AddMembersState) _then) = _$AddMembersStateCopyWithImpl;
@useResult
$Res call({
 List<UiContact> contacts, Set<UiUserId> selectedContacts
});




}
/// @nodoc
class _$AddMembersStateCopyWithImpl<$Res>
    implements $AddMembersStateCopyWith<$Res> {
  _$AddMembersStateCopyWithImpl(this._self, this._then);

  final AddMembersState _self;
  final $Res Function(AddMembersState) _then;

/// Create a copy of AddMembersState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? contacts = null,Object? selectedContacts = null,}) {
  return _then(_self.copyWith(
contacts: null == contacts ? _self.contacts : contacts // ignore: cast_nullable_to_non_nullable
as List<UiContact>,selectedContacts: null == selectedContacts ? _self.selectedContacts : selectedContacts // ignore: cast_nullable_to_non_nullable
as Set<UiUserId>,
  ));
}

}



/// @nodoc


class _AddMembersState implements AddMembersState {
  const _AddMembersState({required final  List<UiContact> contacts, required final  Set<UiUserId> selectedContacts}): _contacts = contacts,_selectedContacts = selectedContacts;
  

 final  List<UiContact> _contacts;
@override List<UiContact> get contacts {
  if (_contacts is EqualUnmodifiableListView) return _contacts;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_contacts);
}

 final  Set<UiUserId> _selectedContacts;
@override Set<UiUserId> get selectedContacts {
  if (_selectedContacts is EqualUnmodifiableSetView) return _selectedContacts;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableSetView(_selectedContacts);
}


/// Create a copy of AddMembersState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$AddMembersStateCopyWith<_AddMembersState> get copyWith => __$AddMembersStateCopyWithImpl<_AddMembersState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _AddMembersState&&const DeepCollectionEquality().equals(other._contacts, _contacts)&&const DeepCollectionEquality().equals(other._selectedContacts, _selectedContacts));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_contacts),const DeepCollectionEquality().hash(_selectedContacts));

@override
String toString() {
  return 'AddMembersState(contacts: $contacts, selectedContacts: $selectedContacts)';
}


}

/// @nodoc
abstract mixin class _$AddMembersStateCopyWith<$Res> implements $AddMembersStateCopyWith<$Res> {
  factory _$AddMembersStateCopyWith(_AddMembersState value, $Res Function(_AddMembersState) _then) = __$AddMembersStateCopyWithImpl;
@override @useResult
$Res call({
 List<UiContact> contacts, Set<UiUserId> selectedContacts
});




}
/// @nodoc
class __$AddMembersStateCopyWithImpl<$Res>
    implements _$AddMembersStateCopyWith<$Res> {
  __$AddMembersStateCopyWithImpl(this._self, this._then);

  final _AddMembersState _self;
  final $Res Function(_AddMembersState) _then;

/// Create a copy of AddMembersState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? contacts = null,Object? selectedContacts = null,}) {
  return _then(_AddMembersState(
contacts: null == contacts ? _self._contacts : contacts // ignore: cast_nullable_to_non_nullable
as List<UiContact>,selectedContacts: null == selectedContacts ? _self._selectedContacts : selectedContacts // ignore: cast_nullable_to_non_nullable
as Set<UiUserId>,
  ));
}


}

// dart format on
