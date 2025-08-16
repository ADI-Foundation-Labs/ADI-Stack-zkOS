use ruint::aliases::B160;
use zk_ee::common_structs::GenericEventContentWithTxRef;
use zk_ee::kv_markers::MAX_EVENT_TOPICS;
use zk_ee::system::errors::system::SystemError;
use zk_ee::system::IOSubsystemExt;
use zk_ee::system::System;
use zk_ee::system::{EthereumLikeTypes, IOTeardown};
use zk_ee::types_config::EthereumIOTypesConfig;
use zk_ee::utils::Bytes32;

pub const DEPOSIT_REQUEST_EIP_7685_TYPE: u8 = 0x00;

pub const DEPOSIT_CONTRACT_ADDRESS: B160 =
    B160::from_limbs([0x9cbe05303d7705fa, 0x219ab540356cbb83, 0x00000000]);

const DEPOSIT_EVENT_SIGNATURE_HASH: Bytes32 =
    Bytes32::from_hex("649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c5");

// it's fully fixed. It's SSZ internally, even though it's not SSZ for the outer vec/list
const DEPOSIT_REQUEST_SERIALIZATION_LEN: usize = 48 + 32 + 8 + 96 + 8;

pub fn eip6110_events_parser<S: EthereumLikeTypes>(
    system: &System<S>,
    requests_hasher: &mut impl crypto::sha256::Digest,
    // requests_hasher: &mut impl MiniDigest,
) -> Result<(), SystemError>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    // we can not easily get the number from one scan, so we will accumulate into hasher directly

    let mut write_prefix = true;
    for event in system.io.events_iterator() {
        if event.address != &DEPOSIT_CONTRACT_ADDRESS {
            continue;
        }
        if event.topics.len() > 0 && event.topics[0] == DEPOSIT_EVENT_SIGNATURE_HASH {
            if write_prefix {
                write_prefix = false;
                requests_hasher.update(&[DEPOSIT_REQUEST_EIP_7685_TYPE]);
            }
            let Ok(_) = validate_and_write_event_data(event, requests_hasher) else {
                panic!("invalid deposit event structure");
            };
        }
    }

    Ok(())
}

fn validate_u16_at_most(input: &[u8; 32], value: u16) -> Result<(), ()> {
    if input[..30].iter().all(|el| *el == 0) == false {
        Err(())
    } else {
        let u16_bytes = [input[30], input[31]];
        if u16::from_be_bytes(u16_bytes) != value {
            Err(())
        } else {
            Ok(())
        }
    }
}

fn validate_and_write_event_data(
    event: GenericEventContentWithTxRef<'_, MAX_EVENT_TOPICS, EthereumIOTypesConfig>,
    requests_hasher: &mut impl crypto::sha256::Digest,
    // requests_hasher: &mut impl MiniDigest,
) -> Result<(), ()> {
    // correctness is asserted on top, so we can lazily parse and write down
    let data = event.data;
    if data.len() != 576 {
        return Err(());
    }
    let mut chunks = data.as_chunks::<32>().0.iter();
    validate_u16_at_most(chunks.next().unwrap(), 160)?;
    validate_u16_at_most(chunks.next().unwrap(), 256)?;
    validate_u16_at_most(chunks.next().unwrap(), 320)?;
    validate_u16_at_most(chunks.next().unwrap(), 384)?;
    validate_u16_at_most(chunks.next().unwrap(), 512)?;
    drop(chunks);

    validate_u16_at_most(data[160..192].try_into().unwrap(), 48)?;
    validate_u16_at_most(data[256..288].try_into().unwrap(), 32)?;
    validate_u16_at_most(data[320..352].try_into().unwrap(), 8)?;
    validate_u16_at_most(data[384..416].try_into().unwrap(), 96)?;
    validate_u16_at_most(data[512..544].try_into().unwrap(), 8)?;

    requests_hasher.update(&data[192..][..48]);
    requests_hasher.update(&data[288..][..32]);
    requests_hasher.update(&data[352..][..8]);
    requests_hasher.update(&data[416..][..96]);
    requests_hasher.update(&data[544..][..8]);

    Ok(())
}
