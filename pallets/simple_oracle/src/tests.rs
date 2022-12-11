use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, pallet_prelude::Get};

const DATA: [u8; 32] = [10; 32];

#[test]
fn push_data() {
	new_test_ext().execute_with(|| {
		assert_ok!(SimpleOracleModule::push_data(
			RuntimeOrigin::signed(Test::DEFAULT_ORACLE_ACCOUNT_ID),
			DATA.into(),
		));

		let storage = SimpleOracleModule::oracle_data().unwrap();
		assert_eq!(storage.as_slice(), [DATA.to_vec()]);

		System::assert_last_event(RuntimeEvent::SimpleOracleModule(Event::Emitted {
			data: DATA.to_vec(),
		}));
	});
}

#[test]
fn push_data_authority_error() {
	new_test_ext().execute_with(|| {
		assert!(SimpleOracleModule::oracle_data().is_none());

		assert_noop!(
			SimpleOracleModule::push_data(RuntimeOrigin::signed(1), DATA.to_vec()),
			Error::<Test>::WrongAuthority
		);
		assert_noop!(
			SimpleOracleModule::push_data(RuntimeOrigin::none(), DATA.to_vec()),
			BadOrigin
		);
		assert_noop!(
			SimpleOracleModule::push_data(RuntimeOrigin::root(), DATA.to_vec()),
			BadOrigin
		);

		assert!(SimpleOracleModule::oracle_data().is_none());
	});
}

#[test]
fn test_lifetime() {
	new_test_ext().execute_with(|| {
		let data_of_moment = |moment: u64| moment.to_be_bytes().to_vec();
		let lifetime = <Test as crate::Config>::OracleDataLifetime::get();
		(0..lifetime).for_each(|moment| {
			Timestamp::set_timestamp(moment);
			assert_ok!(SimpleOracleModule::push_data(
				RuntimeOrigin::signed(Test::DEFAULT_ORACLE_ACCOUNT_ID),
				data_of_moment(moment),
			));
		});

		assert_eq!(
			SimpleOracleModule::oracle_data(),
			Some((0..lifetime).map(data_of_moment).collect::<Vec<_>>())
		);

		(lifetime..lifetime * 2).for_each(|moment| {
			Timestamp::set_timestamp(moment);

			assert_eq!(
				SimpleOracleModule::oracle_data(),
				Some((((moment - lifetime) + 1)..lifetime).map(data_of_moment).collect::<Vec<_>>())
			);
		});

		assert_eq!(SimpleOracleModule::oracle_data(), Some(vec![]));
	});
}

#[test]
fn test_cleanup() {
	new_test_ext().execute_with(|| {
		let data_of_moment = |moment: u64| moment.to_be_bytes().to_vec();
		let lifetime = <Test as crate::Config>::OracleDataLifetime::get();
		(0..lifetime).for_each(|moment| {
			Timestamp::set_timestamp(moment);
			assert_ok!(SimpleOracleModule::push_data(
				RuntimeOrigin::signed(Test::DEFAULT_ORACLE_ACCOUNT_ID),
				data_of_moment(moment),
			));
		});

		let data = Some((0..lifetime).map(data_of_moment).collect::<Vec<_>>());
		assert_eq!(SimpleOracleModule::oracle_data(), data);
		assert_ok!(SimpleOracleModule::clean_outdated_data(RuntimeOrigin::none(),));
		assert_eq!(SimpleOracleModule::oracle_data(), data);

		(lifetime..lifetime * 2).for_each(|moment| {
			Timestamp::set_timestamp(moment);
			assert_ok!(SimpleOracleModule::clean_outdated_data(RuntimeOrigin::none(),));

			assert_eq!(
				SimpleOracleModule::oracle_data(),
				Some((((moment - lifetime) + 1)..lifetime).map(data_of_moment).collect::<Vec<_>>())
			);
		});

		assert_eq!(SimpleOracleModule::oracle_data(), Some(vec![]));
	});
}
