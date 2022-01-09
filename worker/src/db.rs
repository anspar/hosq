use std::{sync::Arc};

// use tokio_postgres::Client;
use web3::types::{Log, H160, U256};
// use web3::helpers::to_string as w3ts;
use crate::DbConn;

fn abi_slice_to_string(b: &[u8])->Result<String, String>{
    // println!("{:?}", b);
    // let offset = U256::from_big_endian(&b[..32]).as_u64();
    if b.len()<=64 {return Err("Invalid abi encoded string".to_owned())}
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

pub async fn update_valid_block(db: Arc<DbConn>, l: Log, chain_id: u64) -> bool {
    if l.data.0.len()<96{
        eprintln!("update_valid_block: data len {:?} !>= 96", l.data.0.len());
        return false
    }

    // let block: i64 = l.block_number.unwrap().low_u64().try_into().unwrap();
    let donor = H160::from_slice(&l.data.0[12..32]);
    let update_block = (&l.block_number.unwrap()).as_u64() as i64;
    let end_block = U256::from_big_endian(&l.data.0[32..64]).as_u64() as i64;
    let block_price = U256::from_big_endian(&l.data.0[64..96]).as_u64() as i64;
    let cid = match abi_slice_to_string(&l.data.0[96..]){
        Ok(v)=>v,
        Err(e)=>{eprintln!("error parsing CID : {:?}", e); return false}
    };
   
    println!("{} -> update_valid_block :: {:?}, {:?}, {:?}, {:?}, {:?} :: {:?}", &chain_id, &donor, &update_block, &end_block, &block_price, &cid, &l.data.0.len());

    db.run(move|client|{
        match client.execute(
            "
            INSERT INTO event_update_valid_block ( donor, update_block, end_block, block_price_gwei, cid, chain_id, ts) 
            values ( LOWER($1::TEXT), $2::BIGINT, $3::BIGINT, $4::BIGINT, $5::TEXT, $6::BIGINT, NOW())
            ON CONFLICT (donor, update_block, end_block, block_price_gwei, cid, chain_id) DO NOTHING
        ",
            &[&format!("{:?}", donor), &update_block, &end_block, &block_price, &cid, &(chain_id as i64)]
        ){
            Ok(_)=>return true,
            Err(e)=>{eprintln!("Error inserting update_valid_block: {:?}", e); return false}
        };
    }).await
}