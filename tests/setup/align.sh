#!/usr/bin/env bash

#SBATCH --output=.slurm/%j-align.out
#SBATCH  -N 1
#SBATCH  -n 16
#SBATCH  -p general
#SBATCH  --mem=64g
#SBATCH  -t 00-04:00:00

set -euo pipefail

ID=$1
GENOMEDIR=$2
GTFFILE=$3

R1=${ID}/${ID}_1.fastq 
R2=${ID}/${ID}_2.fastq 

OUTDIR=$4

STAR --runMode alignReads \
        --runThreadN 16 \
        --readFilesIn ${R1} ${R2} \
        --readFilesCommand zcat \
        --genomeDir ${GENOMEDIR} \
        --sjdbGTFfile ${GTFFILE} \
        --sjdbOverhang 49 \
        --outSAMtype BAM SortedByCoordinate \
        --outSAMmode NoQS \
        --outWigType bedGraph \
        --outWigStrand Stranded \
        --outSAMunmapped Within \
        --outFileNamePrefix ${OUTDIR}/
