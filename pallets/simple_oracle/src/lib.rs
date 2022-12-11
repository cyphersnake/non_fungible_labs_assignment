#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// Module defining storage structures for oracle data
pub mod oracle_data {
	use core::ops::Sub;
	use sp_std::vec::Vec;

	use frame_support::pallet_prelude::{Decode, Encode, Get, RuntimeDebug};
	use scale_info::TypeInfo;

	pub type Data = Vec<u8>;

	#[derive(RuntimeDebug, Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
	pub struct OracleData<MOMENT> {
		data: Data,
		saved_at: MOMENT,
	}

	impl<MOMENT: PartialOrd> PartialOrd for OracleData<MOMENT> {
		fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
			self.saved_at.partial_cmp(&other.saved_at)
		}
	}

	#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, TypeInfo, Default)]
	pub struct OracleStorage<MOMENT>(Vec<OracleData<MOMENT>>);

	#[derive(Debug, PartialEq, Eq)]
	pub enum Error {
		/// An attempt was made to insert outdated data
		///
		/// For simplicity, you can only add data to a storage
		/// if there is no newer data in it.
		AttemptToInsertHistoricalData,
	}

	impl<MOMENT: Sub<MOMENT> + Copy + Ord> OracleStorage<MOMENT>
	where
		<MOMENT as Sub>::Output: PartialOrd<MOMENT>,
	{
		pub fn iter_data<LIFETIME>(&self, now: MOMENT) -> impl Iterator<Item = &[u8]>
		where
			LIFETIME: Get<MOMENT>,
		{
			self.0
				.iter()
				.skip_while(move |oracle_data| now.sub(oracle_data.saved_at).ge(&LIFETIME::get()))
				.map(|oracle_data| oracle_data.data.as_slice())
		}

		/// Delete data from storage if it's alive longer than LIFETIME
		pub fn clean_outdated_data<LIFETIME>(&mut self, now: MOMENT) -> Result<(), Error>
		where
			LIFETIME: Get<MOMENT>,
		{
			if matches!(self.0.last(), Some(OracleData { saved_at, .. }) if saved_at > &now) {
				return Err(Error::AttemptToInsertHistoricalData)
			}

			let point = self.0.partition_point(|data| now.sub(data.saved_at).ge(&LIFETIME::get()));
			self.0.drain(..point);

			Ok(())
		}
		/// Push new data to storage & clean outdated data
		pub fn push<LIFETIME>(&mut self, now: MOMENT, data: Data) -> Result<(), Error>
		where
			LIFETIME: Get<MOMENT>,
		{
			// This call will also check that `now` is not obsolete
			self.clean_outdated_data::<LIFETIME>(now)?;
			self.0.push(OracleData { data, saved_at: now });

			Ok(())
		}
	}

	#[cfg(test)]
	mod oracle_data_test {
		use super::OracleData;
		use sp_core::ConstU64;

		type OracleStorage = super::OracleStorage<u64>;

		#[test]
		fn test_normal_push() {
			let mut storage = OracleStorage::default();
			storage.push::<ConstU64<10>>(0, b"0".to_vec()).unwrap();
			storage.push::<ConstU64<10>>(1, b"1".to_vec()).unwrap();
			storage.push::<ConstU64<10>>(2, b"2".to_vec()).unwrap();

			assert_eq!(
				storage.0.as_slice(),
				[
					OracleData { saved_at: 0, data: b"0".to_vec() },
					OracleData { saved_at: 1, data: b"1".to_vec() },
					OracleData { saved_at: 2, data: b"2".to_vec() }
				]
			);
		}

		#[test]
		fn test_failed_insert() {
			let mut storage = OracleStorage::default();
			storage.push::<ConstU64<10>>(10, b"0".to_vec()).unwrap();
			assert_eq!(
				storage.push::<ConstU64<10>>(0, b"1".to_vec()).unwrap_err(),
				super::Error::AttemptToInsertHistoricalData
			);
		}

		#[test]
		fn test_lifetime() {
			let mut storage = OracleStorage::default();
			storage.push::<ConstU64<10>>(0, b"0".to_vec()).unwrap();
			storage.push::<ConstU64<10>>(10, b"10".to_vec()).unwrap();
			assert_eq!(storage.0.as_slice(), [OracleData { saved_at: 10, data: b"10".to_vec() }]);

			storage.push::<ConstU64<10>>(100, b"100".to_vec()).unwrap();
			assert_eq!(storage.0.as_slice(), [OracleData { saved_at: 100, data: b"100".to_vec() }]);
		}
	}
}

pub mod weights {
	use frame_support::weights::Weight;

	/// Information about pallets pub methods weight
	pub trait WeightInfo {
		const PUSH_WEIGHT: Weight;
		const CLEAN_OUTDATED_DATA_WEIGHT: Weight;
	}

	/// Arbitrary defaults
	impl WeightInfo for () {
		const CLEAN_OUTDATED_DATA_WEIGHT: Weight = Weight::from_ref_time(10_000);
		const PUSH_WEIGHT: Weight = Weight::from_ref_time(10_000);
	}
}

#[frame_support::pallet]
pub mod pallet {
	use sp_std::vec::Vec;

	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	use super::{oracle_data, weights::WeightInfo};

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type DefaultOracleAuthority: Get<Self::AccountId>;
		type OracleDataLifetime: Get<<Self as pallet_timestamp::Config>::Moment>;
		type WeightInfo: WeightInfo;
	}

	/// Storage for events that have been pushed to this oracle.
	/// Stores events for the last hour as required.
	#[pallet::storage]
	pub type EventsStorage<T: Config> =
		StorageValue<_, oracle_data::OracleStorage<<T as pallet_timestamp::Config>::Moment>>;

	impl<T: Config> Pallet<T> {
		/// Storage for events that have been pushed to this oracle.
		/// Stores events for the last hour as required.
        ///
        /// Because there were no additional conditions on the data
        /// access format, we give access only to the data itself
        /// in chronological order.
		pub fn oracle_data() -> Option<Vec<oracle_data::Data>> {
			Some(
				<EventsStorage<T> as frame_support::storage::StorageValue<
					oracle_data::OracleStorage<<T as pallet_timestamp::Config>::Moment>,
				>>::get()?
				.iter_data::<<T as Config>::OracleDataLifetime>(<pallet_timestamp::Pallet<T>>::get())
				.map(|data| data.to_vec())
				.collect(),
			)
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Emitted { data: oracle_data::Data },
	}

	#[pallet::error]
	pub enum Error<T> {
		WrongAuthority,
		AttemptToInsertHistoricalData,
	}

	impl<T> From<oracle_data::Error> for Error<T> {
		fn from(item: oracle_data::Error) -> Self {
			match item {
				oracle_data::Error::AttemptToInsertHistoricalData =>
					Self::AttemptToInsertHistoricalData,
			}
		}
	}

	/// Pallet Struct
	///
	/// Remove storage info, because pallet storage hasn't got stable size
	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Clean outdated data from pallet's storage
		///
		/// Method call allowed for anyone
		#[pallet::weight(<T as Config>::WeightInfo::CLEAN_OUTDATED_DATA_WEIGHT + T::DbWeight::get().reads_writes(1, 1))]
		pub fn clean_outdated_data(_origin: OriginFor<T>) -> DispatchResult {
			<EventsStorage<T>>::try_mutate(|storage| -> Result<(), Error<T>> {
				storage
					.get_or_insert_with(oracle_data::OracleStorage::default)
					.clean_outdated_data::<<T as Config>::OracleDataLifetime>(
					<pallet_timestamp::Pallet<T>>::get(),
				)?;
				Ok(())
			})?;

			Ok(())
		}

		/// Push oracle data
		/// Method deposite [`Event::Emitted`] & store data to pallet storage
		///
		/// Method call allowed only for [`Config::DefaultOracleAuthority`]
		#[pallet::weight(<T as Config>::WeightInfo::PUSH_WEIGHT + T::DbWeight::get().reads_writes(1, 1))]
		pub fn push_data(origin: OriginFor<T>, data: oracle_data::Data) -> DispatchResult {
			if ensure_signed(origin)?.eq(&<T as Config>::DefaultOracleAuthority::get()) {
				Self::deposit_event(Event::Emitted { data: data.clone() });

				<EventsStorage<T>>::try_mutate(|storage| -> Result<(), Error<T>> {
					storage
						.get_or_insert_with(oracle_data::OracleStorage::default)
						.push::<<T as Config>::OracleDataLifetime>(
						<pallet_timestamp::Pallet<T>>::get(),
						data,
					)?;
					Ok(())
				})?;

				Ok(())
			} else {
				Err(Error::<T>::WrongAuthority.into())
			}
		}
	}
}
