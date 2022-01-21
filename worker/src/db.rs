use std::{error::Error, sync::Arc};
// use tokio_postgres::Client;
use web3::types::{Log, H160, U256};
// use web3::helpers::to_string as w3ts;
use crate::types::{DbConn, db::EventUpdateValidBlock, errors::CustomError};

fn abi_slice_to_string(b: &[u8])->Result<String, CustomError>{
    // println!("{:?}", b);
    // let offset = U256::from_big_endian(&b[..32]).as_u64();
    if b.len()<=64 {return Err(CustomError::InvalidAbiString)}
    let mut length = U256::from_big_endian(&b[32..64]).as_u64();
    let mut s = "".to_owned();
    for c in &b[64..]{
        if length==0 {break}
        if *c==0 {continue} 
        s = format!("{}{}", s, &(*c as char));
        length-=1;
    }
    Ok(s)
}

pub async fn update_valid_block(db: Arc<DbConn>, l: Log, chain_id: i64, provider_id: i64) -> Result<(),  Box<dyn Error>> {
    if l.data.0.len()<96{
        error!("update_valid_block: data len {:?} !>= 96", l.data.0.len());
        return Err(Box::new(CustomError::Inequality))
    }

    // let block: i64 = l.block_number.unwrap().low_u64().try_into().unwrap();
    let p_id = U256::from_big_endian(&l.data.0[64..96]).as_u64() as i64;
    if p_id!=provider_id{return Err(Box::new(CustomError::Inequality))}

    let donor = format!("{:?}", H160::from_slice(&l.data.0[12..32]));
    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let end_block = U256::from_big_endian(&l.data.0[32..64]).as_u64() as i64;
    let cid = abi_slice_to_string(&l.data.0[96..])?;
   
    info!("{} -> GOT 'update_valid_block' Event :: {:?}, {:?}, {:?}, {:?}, {:?} :: {:?}", &chain_id, &donor, &update_block, &end_block, &p_id, &cid, &l.data.0.len());

    let res: Result<_, postgres::Error> = db.run(move|client|{
        add_valid_block(client, EventUpdateValidBlock{
           chain_id,
           cid,
           donor,
           update_block,
           end_block,
           manual_add: Option::Some(false)
        })
    }).await;

    match res {
        Ok(_)=>Ok(()),
        Err(e)=>Err(Box::new(e))
    }
}

pub fn add_valid_block(client: &mut postgres::Client, event: EventUpdateValidBlock)->Result<(), postgres::Error>{
    client.execute("
                    INSERT INTO event_update_valid_block ( donor, update_block, end_block, cid, chain_id, manual_add) 
                    values ( LOWER($1::TEXT), $2::BIGINT, $3::BIGINT, $4::TEXT, $5::BIGINT, $6::BOOLEAN)
                    ON CONFLICT (donor, update_block, end_block, cid, chain_id, manual_add) DO NOTHING
                ",
                    &[&event.donor, &event.update_block, &event.end_block, &event.cid, &event.chain_id, &event.manual_add]
                )?;
    Ok(())

    // tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    // println!("from db {}, {}, {}", cid, chain_id, doner);
}

pub fn cid_exists(client: &mut postgres::Client, cid: &str)->Result<bool, postgres::Error>{
    let row = client.query_one("
                    SELECT count(cid) FROM event_update_valid_block WHERE cid=$1::TEXT;
                ",
                    &[&cid]
                )?;
    
    let count: i64 = row.try_get(0)?;
    if count>0{
        return Ok(true);
    }
    Ok(false)
}

pub fn get_max_update_block(client: &mut postgres::Client, table: String, chain_id: i64)-> Result<i64, postgres::Error>{
    let row = client.query_one(&format!("SELECT MAX(update_block) FROM {} WHERE chain_id=$1::BIGINT", table), 
    &[&chain_id])?;
    let maxb: i64 = row.try_get(0)?;
    Ok(maxb)
}